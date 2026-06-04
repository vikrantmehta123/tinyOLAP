//! FullScanExec
//! 
//! Reads all the parts. Only the required columns are read from the parts.
//! One RecordBatch == One part as of now.
//! 
//! TODO: Move towards one RecordBatch == One Granule

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
    storage::{column_reader::ColumnReader, string_column_reader::StringColumnReader},
};

pub struct FullScanExec {
    work_source: Arc<dyn ScanWorkSource>,
    columns: Vec<ColumnSchema>, // which columns to read from the part, in output order
    schema: Arc<Schema>,        // Arrow schema cached once for reuse
}

impl FullScanExec {
    pub fn new(
        work_source: Arc<dyn ScanWorkSource>,
        columns: Vec<ColumnSchema>,
        schema: Arc<Schema>,
    ) -> Self {
        Self {
            work_source,
            columns,
            schema,
        }
    }

    /// For the moment, it is assumed that the work source will hand out an
    /// identifier for a part. For us, it is a path. Then the Scan operator
    /// knows how to read the part based on the identifier.
    ///
    /// Given a path to a part, reads the data and casts it into a RecordBatch
    /// All subsequent query processing operators use RecordBatch as input
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

impl ExecutionPlan for FullScanExec {
    /// Gets the handle from the WorkSource and performs the read operation
    /// For the moment, parallelization is at a granularity of a Part.
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        let part_dir = self.work_source.next_work();
        match part_dir {
            Some(dir) => Some(self.read_part(&dir)),
            None => None,
        }
    }
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let cols: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();
        writeln!(f, "{}FullScan(cols=[{}])", indent, cols.join(", "))
    }
}

/// Pretty Print the operator
impl fmt::Display for FullScanExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
