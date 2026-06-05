//! ZoneMapScanExec
//!
//! Similar to FullScanExec, but this will first check if zone map
//! can be applied and used. Currently, we support zonemaps only on
//! numeric columns. If there is a predicate in the query, the
//! optimizer will convert the FullScan to ZoneMapScan.
//!
//! ZoneMapScan will first read zone-map to determine if we can skip
//! reading the part. If so, reading the part is skipped.
//!
//! Note that zonemaps are at a granularity of parts- not granules.

use std::{fmt, io, sync::Arc};

use arrow::{
    array::{ArrayRef, RecordBatch, StringArray},
    datatypes::Schema,
};
use crate::storage::arrow_mapping::ArrowMappable;
use crate::{
    catalog::schema::{ColumnSchema, DataType},
    execution::{
        executor::{ExecutionError, ExecutionPlan},
        work_source::ScanWorkSource,
    },
    physical_plan::physical_operators::{CmpOp, LiteralValue, LogicalOp, PhysicalExpr},
    storage::{
        column_reader::ColumnReader,
        string_column_reader::StringColumnReader,
        zone_map::{ZoneMap, read_zone_map},
    },
};

enum ColumnReaderKind {
    Numeric(ColumnReader),
    Str(StringColumnReader),
}

/// Skip the Part Level Reading.
/// Filtering is not done at scan level.
pub struct ZoneMapScanExec {
    work_source: Arc<dyn ScanWorkSource>,
    columns: Vec<ColumnSchema>,   // which columns to read, in output order
    schema: Arc<Schema>,          // Arrow schema cached once for reuse
    skip_predicate: PhysicalExpr, // the predicate used to skip parts from being read

    // the index of the granule that the readers are currently reading
    // each thread is expected to have its own copy since no two threads
    // will be reading the same part as guaranteed by the work source
    granule_idx: usize,

    // The number of granules in the current part
    granule_count: usize,

    // Keep the readers open per part
    readers: Vec<ColumnReaderKind>,
}

impl ZoneMapScanExec {
    pub fn new(
        work_source: Arc<dyn ScanWorkSource>,
        columns: Vec<ColumnSchema>,
        schema: Arc<Schema>,
        skip_predicate: PhysicalExpr,
    ) -> Self {
        Self {
            work_source,
            columns,
            schema,
            skip_predicate,
            granule_idx: 0,
            granule_count: 0,
            readers: Vec::new(),
        }
    }
}

impl ExecutionPlan for ZoneMapScanExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        while self.readers.is_empty() {
            let part_dir = self.work_source.next_work()?;

            let zone_map = match read_zone_map(&part_dir) {
                Ok(zm) => zm,
                Err(_) => ZoneMap::default(),
            };
            if zone_map_can_skip(&self.skip_predicate, &zone_map) {
                continue;
            }

            let readers: io::Result<Vec<ColumnReaderKind>> = self
                .columns
                .iter()
                .map(|col| {
                    Ok(match col.data_type {
                        DataType::Str => {
                            ColumnReaderKind::Str(StringColumnReader::open(&part_dir, &col.name)?)
                        }
                        _ => ColumnReaderKind::Numeric(ColumnReader::open(&part_dir, &col.name)?),
                    })
                })
                .collect();

            self.readers = match readers {
                Ok(r) => r,
                Err(e) => return Some(Err(e.into())),
            };
            self.granule_count = match &self.readers[0] {
                ColumnReaderKind::Numeric(r) => r.granule_count(),
                ColumnReaderKind::Str(r) => r.granule_count(),
            };
            self.granule_idx = 0;
        }

        let idx = self.granule_idx;

        let arrays = self
            .columns
            .iter()
            .zip(self.readers.iter_mut())
            .map(|(col, reader)| -> Result<ArrayRef, ExecutionError> {
                Ok(match reader {
                    ColumnReaderKind::Numeric(r) => match col.data_type {
                        DataType::I8 => i8::into_array(r.read_granule::<i8>(idx)?),
                        DataType::I16 => i16::into_array(r.read_granule::<i16>(idx)?),
                        DataType::I32 => i32::into_array(r.read_granule::<i32>(idx)?),
                        DataType::I64 => i64::into_array(r.read_granule::<i64>(idx)?),
                        DataType::U8 => u8::into_array(r.read_granule::<u8>(idx)?),
                        DataType::U16 => u16::into_array(r.read_granule::<u16>(idx)?),
                        DataType::U32 => u32::into_array(r.read_granule::<u32>(idx)?),
                        DataType::U64 => u64::into_array(r.read_granule::<u64>(idx)?),
                        DataType::F32 => f32::into_array(r.read_granule::<f32>(idx)?),
                        DataType::F64 => f64::into_array(r.read_granule::<f64>(idx)?),
                        DataType::Bool => bool::into_array(r.read_granule::<bool>(idx)?),
                        DataType::Str => unreachable!(),
                    },
                    ColumnReaderKind::Str(r) => {
                        let v = r.read_granule(idx)?;
                        Arc::new(StringArray::from(v))
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>();

        let arrays = match arrays {
            Ok(a) => a,
            Err(e) => return Some(Err(e)),
        };

        self.granule_idx += 1;
        if self.granule_idx >= self.granule_count {
            self.readers.clear();
        }

        Some(RecordBatch::try_new(self.schema.clone(), arrays).map_err(ExecutionError::from))
    }

    fn fmt_indented(&self, f: &mut std::fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let cols: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();
        writeln!(f, "{}ZoneMapScan(cols=[{}])", indent, cols.join(", "))
    }
}

/// Pretty Print the operator
impl fmt::Display for ZoneMapScanExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

fn zone_map_can_skip(predicate: &PhysicalExpr, zone_map: &ZoneMap) -> bool {
    match predicate {
        PhysicalExpr::Compare { left, op, right } => match (left.as_ref(), right.as_ref()) {
            (PhysicalExpr::Column(col_name), PhysicalExpr::Literal(lit)) => {
                match zone_map.get(col_name) {
                    None => false,
                    Some(col_zone) => {
                        // Verify that there are entries in the zonemap.
                        // In a zone map, entries are supposed to be
                        // present always, but in case of corruption, etc.,
                        // this is a safety check.
                        let Some(_) = col_zone.entries.first() else {
                            return false;
                        };
                        let e = &col_zone.entries[0];
                        match (col_zone.type_tag, lit) {
                            (1..=4, LiteralValue::I64(v)) => {
                                let min = i64::from_le_bytes(e.min_bytes);
                                let max = i64::from_le_bytes(e.max_bytes);
                                cmp_can_skip_signed(op, min, max, *v)
                            }
                            (5..=8, LiteralValue::U64(v)) => {
                                let min = u64::from_le_bytes(e.min_bytes);
                                let max = u64::from_le_bytes(e.max_bytes);
                                cmp_can_skip_unsigned(op, min, max, *v)
                            }
                            (9..=10, LiteralValue::F64(v)) => {
                                let min = f64::from_bits(u64::from_le_bytes(e.min_bytes));
                                let max = f64::from_bits(u64::from_le_bytes(e.max_bytes));
                                cmp_can_skip_float(op, min, max, *v)
                            }
                            _ => false,
                        }
                    }
                }
            }
            _ => false,
        },
        PhysicalExpr::Logical { left, op, right } => match op {
            LogicalOp::And => {
                zone_map_can_skip(left, zone_map) || zone_map_can_skip(right, zone_map)
            }
            LogicalOp::Or => {
                zone_map_can_skip(left, zone_map) && zone_map_can_skip(right, zone_map)
            }
        },
        _ => false,
    }
}

fn cmp_can_skip_signed(op: &CmpOp, min: i64, max: i64, v: i64) -> bool {
    match op {
        CmpOp::Gt => max <= v,
        CmpOp::GtEq => max < v,
        CmpOp::Lt => min >= v,
        CmpOp::LtEq => min > v,
        CmpOp::Eq => v < min || v > max,
        CmpOp::NotEq => min == max && min == v,
    }
}

fn cmp_can_skip_unsigned(op: &CmpOp, min: u64, max: u64, v: u64) -> bool {
    match op {
        CmpOp::Gt => max <= v,
        CmpOp::GtEq => max < v,
        CmpOp::Lt => min >= v,
        CmpOp::LtEq => min > v,
        CmpOp::Eq => v < min || v > max,
        CmpOp::NotEq => min == max && min == v,
    }
}

fn cmp_can_skip_float(op: &CmpOp, min: f64, max: f64, v: f64) -> bool {
    match op {
        CmpOp::Gt => max <= v,
        CmpOp::GtEq => max < v,
        CmpOp::Lt => min >= v,
        CmpOp::LtEq => min > v,
        CmpOp::Eq => v < min || v > max,
        CmpOp::NotEq => min == max && min == v,
    }
}
