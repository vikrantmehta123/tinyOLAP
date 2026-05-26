use std::{
    fmt, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicUsize, Ordering::Relaxed}}
};

use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::Schema,
};

use crate::{
    catalog::schema::{ColumnSchema, DataType},
    execution::executor::{ExecutionError, ExecutionPlan}, storage::{column_reader::ColumnReader, string_column_reader::StringColumnReader},
};

pub trait ScanWorkSource {
    fn next_work(&self) -> Option<PathBuf>;
}

pub struct PartWorkSource{
    parts: Vec<PathBuf>,
    next: AtomicUsize,
}

impl PartWorkSource {
    pub fn new(parts: Vec<PathBuf>) -> Self {
        Self {
            parts, 
            next: AtomicUsize::new(0),
        }
    }
}

impl ScanWorkSource for PartWorkSource { 
    fn next_work(&self) -> Option<PathBuf> {
        let val = self.next.fetch_add(1, Relaxed);
        self.parts.get(val).cloned()
    }
}

pub struct FullScanExec {
    work_source: Arc<dyn ScanWorkSource>,
    columns: Vec<ColumnSchema>, // which columns to read, in output order
    schema: Arc<Schema>, // Arrow schema cached once for reuse
}

impl FullScanExec {
    pub fn new(work_source: Arc<dyn ScanWorkSource>, columns: Vec<ColumnSchema>, schema: Arc<Schema>) -> Self {
        Self {
            work_source, 
            columns,
            schema,
        }
    }

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
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        let part_dir = self.work_source.next_work();
        match part_dir {
            Some(dir) => Some(self.read_part(&dir)),
            None => None

        }
    }
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let cols: Vec<&str> = self.columns.iter().map(|c| c.name.as_str()).collect();
        writeln!(f, "{}FullScan(cols=[{}])", indent, cols.join(", "))
    }
}


impl fmt::Display for FullScanExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
