use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::{Field, Schema},
};

use crate::{
    catalog::schema::{ColumnSchema, DataType, TableSchema},
    execution::executor::{ExecutionError, ExecutionPlan}, storage::{column_reader::ColumnReader, string_column_reader::StringColumnReader},
};

pub struct FullScanExec {
    parts: Vec<PathBuf>,        // remaining part directories, popped from the back
    columns: Vec<ColumnSchema>, // which columns to read, in output order
    schema: Arc<arrow::datatypes::Schema>, // Arrow schema cached once for reuse
}

impl FullScanExec {
    pub fn new(
        table_dir: &Path,
        column_names: Vec<String>,
        table_schema: &TableSchema,
    ) -> Result<Self, ExecutionError> {
        let mut parts: Vec<PathBuf> = fs::read_dir(table_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map_or(false, |n| n.starts_with("part_"))
            })
            .collect();
        parts.sort();
        parts.reverse();

        let columns: Vec<ColumnSchema> = column_names
            .iter()
            .map(|name| {
                table_schema
                    .columns
                    .iter()
                    .find(|c| c.name == *name)
                    .cloned()
                    .ok_or_else(|| ExecutionError::InvalidData(format!("unknown column: {}", name)))
            })
            .collect::<Result<_, _>>()?;

        // 3. Build the Arrow schema once and cache it. Every RecordBatch this
        //    scan emits shares this exact Arc<Schema> — no per-batch alloc.
        let fields: Vec<Field> = columns
            .iter()
            .map(|c| Field::new(&c.name, to_arrow_dt(&c.data_type), false))
            .collect();
        let schema = Arc::new(Schema::new(fields));

        Ok(Self {
            parts,
            columns,
            schema,
        })
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

// TODO: Find a better place to move this function
fn to_arrow_dt(dt: &DataType) -> arrow::datatypes::DataType {
    use arrow::datatypes::DataType as ArrowDt;
    match dt {
        DataType::I8 => ArrowDt::Int8,
        DataType::I16 => ArrowDt::Int16,
        DataType::I32 => ArrowDt::Int32,
        DataType::I64 => ArrowDt::Int64,
        DataType::U8 => ArrowDt::UInt8,
        DataType::U16 => ArrowDt::UInt16,
        DataType::U32 => ArrowDt::UInt32,
        DataType::U64 => ArrowDt::UInt64,
        DataType::F32 => ArrowDt::Float32,
        DataType::F64 => ArrowDt::Float64,
        DataType::Bool => ArrowDt::Boolean,
        DataType::Str => ArrowDt::Utf8,
    }
}

impl ExecutionPlan for FullScanExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        let part_dir = self.parts.pop()?;
        Some(self.read_part(&part_dir))
    }
}
