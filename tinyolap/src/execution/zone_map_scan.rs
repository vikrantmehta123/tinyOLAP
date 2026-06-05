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

use std::{fmt, path::Path, sync::Arc};

use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::Schema,
};

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

/// Skip the Part Level Reading.
/// Filtering is not done at scan level.
pub struct ZoneMapScanExec {
    work_source: Arc<dyn ScanWorkSource>,
    columns: Vec<ColumnSchema>,   // which columns to read, in output order
    schema: Arc<Schema>,          // Arrow schema cached once for reuse
    skip_predicate: PhysicalExpr, // the predicate used to skip parts from being read
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
        }
    }

    /// Returns raw `RecordBatch`es for parts that pass the zone-map check.
    /// Does NOT apply the predicate at the row level — the caller must wrap
    /// this in a `FilterExec` to produce correct results.
    fn read_part(&self, part_dir: &Path) -> Result<RecordBatch, ExecutionError> {
        let arrays: Vec<ArrayRef> = self
            .columns
            .iter()
            .map(|col| -> Result<ArrayRef, ExecutionError> {
                Ok(match col.data_type {
                    DataType::I8 => ColumnReader::open(part_dir, &col.name)?.read_all::<i8>()?,
                    DataType::I16 => ColumnReader::open(part_dir, &col.name)?.read_all::<i16>()?,
                    DataType::I32 => ColumnReader::open(part_dir, &col.name)?.read_all::<i32>()?,
                    DataType::I64 => ColumnReader::open(part_dir, &col.name)?.read_all::<i64>()?,
                    DataType::U8 => ColumnReader::open(part_dir, &col.name)?.read_all::<u8>()?,
                    DataType::U16 => ColumnReader::open(part_dir, &col.name)?.read_all::<u16>()?,
                    DataType::U32 => ColumnReader::open(part_dir, &col.name)?.read_all::<u32>()?,
                    DataType::U64 => ColumnReader::open(part_dir, &col.name)?.read_all::<u64>()?,
                    DataType::F32 => ColumnReader::open(part_dir, &col.name)?.read_all::<f32>()?,
                    DataType::F64 => ColumnReader::open(part_dir, &col.name)?.read_all::<f64>()?,
                    DataType::Bool => {
                        ColumnReader::open(part_dir, &col.name)?.read_all::<bool>()?
                    }
                    DataType::Str => StringColumnReader::open(part_dir, &col.name)?.read_all()?,
                })
            })
            .collect::<Result<_, _>>()?;

        Ok(RecordBatch::try_new(self.schema.clone(), arrays)?)
    }
}

impl ExecutionPlan for ZoneMapScanExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        // In FullScan, we don't need to have loop because FullScan will always have the next part
        // or it will be end of stream. But in ZoneMapScan, we may have to loop until we find a
        // part to read.
        loop {
            let part_dir = self.work_source.next_work();
            match part_dir {
                Some(dir) => {
                    let zone_map = match read_zone_map(&dir) {
                        Ok(zm) => zm,

                        // Some error due to which zone map couldn't be read.
                        // This is not supposed to happen- as Zone Maps are always supposed
                        // to exist for a given part. But say this happens, then read the part
                        // just to be safe.
                        Err(_) => return Some(self.read_part(&dir)),
                    };
                    if zone_map_can_skip(&self.skip_predicate, &zone_map) {
                        continue;
                    }

                    return Some(self.read_part(&dir));
                }
                None => {
                    return None;
                }
            }
        }
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
