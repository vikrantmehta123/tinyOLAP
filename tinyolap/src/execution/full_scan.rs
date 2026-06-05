//! FullScanExec
//!
//! Reads all the parts. Only the required columns are read from the parts.
//! One RecordBatch == One part as of now.

use crate::storage::arrow_mapping::ArrowMappable;
use std::{fmt, io, sync::Arc};

use arrow::{
    array::{ArrayRef, RecordBatch, StringArray},
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

enum ColumnReaderKind {
    Numeric(ColumnReader),
    Str(StringColumnReader),
}

pub struct FullScanExec {
    work_source: Arc<dyn ScanWorkSource>,
    columns: Vec<ColumnSchema>, // which columns to read from the part, in output order
    schema: Arc<Schema>,        // Arrow schema cached once for reuse

    // the index of the granule that the readers are currently reading
    // each thread is expected to have its own copy since no two threads
    // will be reading the same part as guaranteed by the work source
    granule_idx: usize,

    // The number of granules in the current part
    granule_count: usize,

    // Keep the readers open per part
    readers: Vec<ColumnReaderKind>,
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
            granule_idx: 0,
            granule_count: 0,
            readers: Vec::new(),
        }
    }
}

impl ExecutionPlan for FullScanExec {
    /// Gets the handle from the WorkSource and performs the read operation
    /// For parallelization, we still want threads to be tied to a particular
    /// part to avoid open/close calls for files. However, when emitting the
    /// next batch, we want them to emit smaller batches than entire parts,
    /// because parts can be big. So, each thread still reads a part but
    /// emits a few granules as a batch, instead of entire part.
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        if self.readers.is_empty() {
            let part_dir = self.work_source.next_work()?;

            let readers: io::Result<Vec<ColumnReaderKind>> = self.columns.iter().map(|col| {
                Ok(match col.data_type {
                    DataType::Str => ColumnReaderKind::Str(StringColumnReader::open(&part_dir, &col.name)?),
                    _             => ColumnReaderKind::Numeric(ColumnReader::open(&part_dir, &col.name)?),
                })
            }).collect();

            self.readers = match readers {
                Ok(r) => r,
                Err(e) => return Some(Err(e.into())),
            };

            self.granule_count = match &self.readers[0] {
                ColumnReaderKind::Numeric(r) => r.granule_count(),
                ColumnReaderKind::Str(r)     => r.granule_count(),
            };
            self.granule_idx = 0;
        }

        let idx = self.granule_idx;

        let arrays = self.columns.iter().zip(self.readers.iter_mut())
            .map(|(col, reader)| -> Result<ArrayRef, ExecutionError> {
                Ok(match reader {
                    ColumnReaderKind::Numeric(r) => match col.data_type {
                        DataType::I8   => i8::into_array(r.read_granule::<i8>(idx)?),
                        DataType::I16  => i16::into_array(r.read_granule::<i16>(idx)?),
                        DataType::I32  => i32::into_array(r.read_granule::<i32>(idx)?),
                        DataType::I64  => i64::into_array(r.read_granule::<i64>(idx)?),
                        DataType::U8   => u8::into_array(r.read_granule::<u8>(idx)?),
                        DataType::U16  => u16::into_array(r.read_granule::<u16>(idx)?),
                        DataType::U32  => u32::into_array(r.read_granule::<u32>(idx)?),
                        DataType::U64  => u64::into_array(r.read_granule::<u64>(idx)?),
                        DataType::F32  => f32::into_array(r.read_granule::<f32>(idx)?),
                        DataType::F64  => f64::into_array(r.read_granule::<f64>(idx)?),
                        DataType::Bool => bool::into_array(r.read_granule::<bool>(idx)?),
                        DataType::Str  => unreachable!(),
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
