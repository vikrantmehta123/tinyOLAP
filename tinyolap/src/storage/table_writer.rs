use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use arrow::array::{
    Array, BooleanArray, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
    RecordBatch, StringArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};

use arrow::compute::{SortColumn, lexsort_to_indices, take};

use arrow::datatypes::DataType as ArrowDt;
use rayon::prelude::*;

use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};
use crate::storage::column_writer::write_column;
use crate::storage::string_column_writer::write_string_column;
use crate::storage::zone_map::{EncodedZoneMapEntry, ZoneEntry, ZoneMapEntry, write_zone_map};

use crate::encoding::{Codec, StringCodec};
pub struct PartMetadata {
    pub part_id: u32,
    pub rows: u64,
}

pub struct TableWriter {
    schema: TableSchema,
    table_dir: PathBuf,
    next_part_id: AtomicU32,
}

impl TableWriter {
    pub fn open(table_dir: PathBuf) -> io::Result<Self> {
        let schema = TableSchema::open(&table_dir)?;
        let next_part_id = scan_next_part_id(&table_dir)?;
        Ok(Self {
            schema,
            table_dir,
            next_part_id: AtomicU32::new(next_part_id),
        })
    }

    pub fn insert(&self, batch: RecordBatch) -> io::Result<PartMetadata> {
        // Validate num columns in batch and in schema. NULL values are not supported.
        // So we assume this is enough.
        if batch.num_columns() != self.schema.columns.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "expected {} chunks, got {}",
                    self.schema.columns.len(),
                    batch.num_columns()
                ),
            ));
        }

        // Validate the type of each column
        let row_count = batch.num_rows();
        for (array, col) in batch.columns().iter().zip(self.schema.columns.iter()) {
            check_type(array.as_ref(), col)?;
        }

        // Sort the batch based on the sort key and recast the batch
        // In arrow, we compute first the sorted permutation and then apply
        // that permutation to each column. There's no: batch.sort(key=some_key)
        // We assume sort_key is always non-empty.
        let sort_cols: Vec<SortColumn> = self
            .schema
            .sort_key
            .iter()
            .map(|&col_idx| SortColumn {
                options: None,
                values: batch.column(col_idx).clone(),
            })
            .collect();

        let indices = lexsort_to_indices(&sort_cols, None)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let sorted_cols = batch
            .columns()
            .iter()
            .map(|c| take(c, &indices, None))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let batch = RecordBatch::try_new(batch.schema(), sorted_cols)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // ---- 2. Reserve a part id and create a tmp dir. ----
        let part_id = self.next_part_id.fetch_add(1, Ordering::SeqCst);
        let tmp_dir = self.table_dir.join(format!("tmp_part_{:05}", part_id));
        let final_dir = TableSchema::part_dir(&self.table_dir, part_id);
        fs::create_dir_all(&tmp_dir)?;

        // ---- 3. Write all columns in parallel. ----
        let result: io::Result<Vec<Option<EncodedZoneMapEntry>>> = batch
            .columns()
            .par_iter()
            .zip(self.schema.columns.par_iter())
            .map(|(chunk, col)| write_one_column(&tmp_dir, col, chunk))
            .collect();

        let entries: Vec<EncodedZoneMapEntry> = match result {
            Ok(per_column) => per_column.into_iter().flatten().collect(),
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err(e);
            }
        };

        // Write part.zonemap into the tmp dir; the rename below makes it
        // atomic with the rest of the part.
        if let Err(e) = write_zone_map(&tmp_dir, &entries) {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }

        // ---- 4. Atomic-ish finalize: rename tmp -> part_NNNNN. ----
        fs::rename(&tmp_dir, &final_dir)?;
        fs::File::open(&self.table_dir)?.sync_all()?;

        Ok(PartMetadata {
            part_id,
            rows: row_count as u64,
        })
    }
}

fn check_type(array: &dyn Array, col: &ColumnSchema) -> io::Result<()> {
    let ok = matches!(
        (array.data_type(), &col.data_type),
        (ArrowDt::Int8, DataType::I8)
            | (ArrowDt::Int16, DataType::I16)
            | (ArrowDt::Int32, DataType::I32)
            | (ArrowDt::Int64, DataType::I64)
            | (ArrowDt::UInt8, DataType::U8)
            | (ArrowDt::UInt16, DataType::U16)
            | (ArrowDt::UInt32, DataType::U32)
            | (ArrowDt::UInt64, DataType::U64)
            | (ArrowDt::Float32, DataType::F32)
            | (ArrowDt::Float64, DataType::F64)
            | (ArrowDt::Boolean, DataType::Bool)
            | (ArrowDt::Utf8, DataType::Str)
    );
    if !ok {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "column '{}': chunk type does not match schema type {:?}",
                col.name, col.data_type
            ),
        ));
    }
    Ok(())
}

fn widen_signed<T: Copy + Into<i64>>(z: ZoneMapEntry<T>) -> ([u8; 8], [u8; 8]) {
    let min: i64 = z.min.into();
    let max: i64 = z.max.into();
    (min.to_le_bytes(), max.to_le_bytes())
}

fn widen_unsigned<T: Copy + Into<u64>>(z: ZoneMapEntry<T>) -> ([u8; 8], [u8; 8]) {
    let min: u64 = z.min.into();
    let max: u64 = z.max.into();
    (min.to_le_bytes(), max.to_le_bytes())
}

fn widen_float<T: Copy + Into<f64>>(z: ZoneMapEntry<T>) -> ([u8; 8], [u8; 8]) {
    let min: f64 = z.min.into();
    let max: f64 = z.max.into();
    (min.to_bits().to_le_bytes(), max.to_bits().to_le_bytes())
}

fn write_one_column(
    part_dir: &Path,
    col: &ColumnSchema,
    array: &dyn Array,
) -> io::Result<Option<EncodedZoneMapEntry>> {
    let codec = codec_for(col);

    let bytes = match col.data_type {
        DataType::I8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            write_column::<i8>(part_dir, &col.name, arr.values(), codec)?.map(widen_signed)
        }
        DataType::I16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            write_column::<i16>(part_dir, &col.name, arr.values(), codec)?.map(widen_signed)
        }
        DataType::I32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            write_column::<i32>(part_dir, &col.name, arr.values(), codec)?.map(widen_signed)
        }
        DataType::I64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            write_column::<i64>(part_dir, &col.name, arr.values(), codec)?.map(widen_signed)
        }
        DataType::U8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            write_column::<u8>(part_dir, &col.name, arr.values(), codec)?.map(widen_unsigned)
        }
        DataType::U16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            write_column::<u16>(part_dir, &col.name, arr.values(), codec)?.map(widen_unsigned)
        }
        DataType::U32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            write_column::<u32>(part_dir, &col.name, arr.values(), codec)?.map(widen_unsigned)
        }
        DataType::U64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            write_column::<u64>(part_dir, &col.name, arr.values(), codec)?.map(widen_unsigned)
        }
        DataType::F32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            write_column::<f32>(part_dir, &col.name, arr.values(), codec)?.map(widen_float)
        }
        DataType::F64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            write_column::<f64>(part_dir, &col.name, arr.values(), codec)?.map(widen_float)
        }
        DataType::Bool => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            // TODO(perf): teach write_column to take a BooleanBuffer to avoid this unpack.
            let bools: Vec<bool> = arr.iter().map(|opt| opt.unwrap_or(false)).collect();
            write_column::<bool>(part_dir, &col.name, &bools, codec)?;
            None
        }
        DataType::Str => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            // TODO(TASK-005): teach write_string_column to take iter of &str, avoid this Vec<String>.
            let strs: Vec<String> = arr
                .iter()
                .map(|opt| opt.unwrap_or("").to_string())
                .collect();
            write_string_column(part_dir, &col.name, &strs, StringCodec::Plain)?;
            None
        }
    };

    Ok(bytes.map(|(min_bytes, max_bytes)| EncodedZoneMapEntry {
        col_name: col.name.clone(),
        type_tag: col.data_type.type_tag(),
        entry: ZoneEntry {
            min_bytes,
            max_bytes,
        },
    }))
}

/// Codec selection lives here so column_writer stays type-blind.
/// Today: Plain for everything. Future: read from ColumnSchema once the schema
/// carries codec metadata (e.g. Delta for timestamp columns).
fn codec_for(col: &ColumnSchema) -> Codec {
    match col.data_type {
        DataType::I16
        | DataType::I32
        | DataType::I64
        | DataType::U16
        | DataType::U32
        | DataType::U64 => Codec::Delta,
        _ => Codec::Plain,
    }
}

fn scan_next_part_id(table_dir: &Path) -> io::Result<u32> {
    let mut max_id: i64 = -1;
    for entry in fs::read_dir(table_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("part_") {
            if let Ok(id) = rest.parse::<u32>() {
                max_id = max_id.max(id as i64);
            }
        }
    }
    Ok((max_id + 1) as u32)
}
