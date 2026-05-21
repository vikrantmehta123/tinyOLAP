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
    count: Vec<u64>,
}

impl CountAgg {
    pub fn new() -> Self {
        Self { count: vec![] }
    }
}

impl Aggregator for CountAgg {
    fn update(&mut self, chunk: &ColumnChunk, group_ids: &[u32], n_groups: usize) -> Result<(), ExecutionError> {
        if self.count.len() < n_groups { self.count.resize(n_groups, 0);}
        
        for &gid in group_ids {
            self.count[gid as usize] += 1;
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        ColumnChunk::U64(self.count.clone())
    }

    fn output_type(&self) -> DataType {
        DataType::U64
    }
}
