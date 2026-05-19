use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;

use crate::storage::column_chunk::ColumnChunk;
use crate::storage::column_writer::{write_column};
use crate::storage::schema::{ColumnDef, DataType, TableDef};
use crate::storage::string_column_writer::write_string_column;
use crate::storage::zone_map::{ZoneMapEntry, EncodedZoneMapEntry, write_zone_map};

use crate::encoding::{Codec, StringCodec};
pub struct PartMetadata {
    pub part_id: u32,
    pub rows: u64,
}

pub struct TableWriter {
    schema: TableDef,
    table_dir: PathBuf,
    next_part_id: AtomicU32,
}

impl TableWriter {
    pub fn open(table_dir: PathBuf) -> io::Result<Self> {
        let schema = TableDef::open(&table_dir)?;
        let next_part_id = scan_next_part_id(&table_dir)?;
        Ok(Self {
            schema,
            table_dir,
            next_part_id: AtomicU32::new(next_part_id),
        })
    }

    pub fn insert(&self, chunks: Vec<ColumnChunk>) -> io::Result<PartMetadata> {
        // ---- 1. Validate shape & types up front, before any I/O. ----
        if chunks.len() != self.schema.columns.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "expected {} chunks, got {}",
                    self.schema.columns.len(),
                    chunks.len()
                ),
            ));
        }
        let row_count = chunks.first().map(|c| c.len()).unwrap_or(0);
        for (i, chunk) in chunks.iter().enumerate() {
            if chunk.len() != row_count {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("column {i}: row count mismatch"),
                ));
            }
            check_type(chunk, &self.schema.columns[i])?;
        }

        // ---- 2. Reserve a part id and create a tmp dir. ----
        let part_id = self.next_part_id.fetch_add(1, Ordering::SeqCst);
        let tmp_dir = self.table_dir.join(format!("tmp_part_{:05}", part_id));
        let final_dir = TableDef::part_dir(&self.table_dir, part_id);
        fs::create_dir_all(&tmp_dir)?;

        // ---- 3. Write all columns in parallel. ----
        let result: io::Result<Vec<Option<EncodedZoneMapEntry>>> = chunks
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

fn check_type(chunk: &ColumnChunk, col: &ColumnDef) -> io::Result<()> {
    let ok = matches!(
        (chunk, &col.data_type),
        (ColumnChunk::I8(_),   DataType::I8)
        | (ColumnChunk::I16(_),  DataType::I16)
        | (ColumnChunk::I32(_),  DataType::I32)
        | (ColumnChunk::I64(_),  DataType::I64)
        | (ColumnChunk::U8(_),   DataType::U8)
        | (ColumnChunk::U16(_),  DataType::U16)
        | (ColumnChunk::U32(_),  DataType::U32)
        | (ColumnChunk::U64(_),  DataType::U64)
        | (ColumnChunk::F32(_),  DataType::F32)
        | (ColumnChunk::F64(_),  DataType::F64)
        | (ColumnChunk::Bool(_), DataType::Bool)
        | (ColumnChunk::Str(_),  DataType::Str)
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
    col: &ColumnDef,
    chunk: &ColumnChunk,
) -> io::Result<Option<EncodedZoneMapEntry>> {
    let codec = codec_for(col);

    let bytes = match chunk {
        ColumnChunk::I8(v)   => write_column::<i8>(part_dir,  &col.name, v, codec)?.map(widen_signed),
        ColumnChunk::I16(v)  => write_column::<i16>(part_dir, &col.name, v, codec)?.map(widen_signed),
        ColumnChunk::I32(v)  => write_column::<i32>(part_dir, &col.name, v, codec)?.map(widen_signed),
        ColumnChunk::I64(v)  => write_column::<i64>(part_dir, &col.name, v, codec)?.map(widen_signed),
        ColumnChunk::U8(v)   => write_column::<u8>(part_dir,  &col.name, v, codec)?.map(widen_unsigned),
        ColumnChunk::U16(v)  => write_column::<u16>(part_dir, &col.name, v, codec)?.map(widen_unsigned),
        ColumnChunk::U32(v)  => write_column::<u32>(part_dir, &col.name, v, codec)?.map(widen_unsigned),
        ColumnChunk::U64(v)  => write_column::<u64>(part_dir, &col.name, v, codec)?.map(widen_unsigned),
        ColumnChunk::F32(v)  => write_column::<f32>(part_dir, &col.name, v, codec)?.map(widen_float),
        ColumnChunk::F64(v)  => write_column::<f64>(part_dir, &col.name, v, codec)?.map(widen_float),
        ColumnChunk::Bool(v) => {
            write_column::<bool>(part_dir, &col.name, v, codec)?;
            None
        }
        ColumnChunk::Str(v) => {
            write_string_column(part_dir, &col.name, v, StringCodec::Plain)?;
            None
        }
    };

    Ok(bytes.map(|(min_bytes, max_bytes)| EncodedZoneMapEntry {
        col_name: col.name.clone(),
        type_tag: col.data_type.type_tag(),
        min_bytes,
        max_bytes,
    }))
}

/// Codec selection lives here so column_writer stays type-blind.
/// Today: Plain for everything. Future: read from ColumnDef once the schema
/// carries codec metadata (e.g. Delta for timestamp columns).
fn codec_for(col: &ColumnDef) -> Codec {
    match col.data_type {
        DataType::I16 | DataType::I32 | DataType::I64
        | DataType::U16 | DataType::U32 | DataType::U64 => Codec::Delta,
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
