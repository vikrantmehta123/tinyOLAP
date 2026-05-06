//! Count aggregate over any column type.
//!
//! Values are ignored — only the row count matters. Output is always U64.
//! count(*) is rejected at the parser layer; this impl only sees count(col).
//! count() is also rejected. NULL support is not there.

use crate::aggregator::Aggregator;
use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

pub struct CountAgg {
    count: u64,
}

impl CountAgg {
    pub fn new() -> Self {
        Self { count: 0 }
    }
}

impl Aggregator for CountAgg {
    fn update(&mut self, chunk: &ColumnChunk) -> Result<(), ExecutionError> {
        self.count += chunk.len() as u64;
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        ColumnChunk::U64(vec![self.count])
    }

    fn output_type(&self) -> DataType {
        DataType::U64
    }
}
