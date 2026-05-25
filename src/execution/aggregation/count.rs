use std::sync::Arc;

use arrow::{array::{ArrayRef, RecordBatch, UInt64Array}, datatypes::{DataType, Field}};

use crate::execution::{aggregation::Accumulator, executor::{ExecutionError}};

pub struct CountAccumulator {
    count: u64, 
    column_name: String,
}

impl CountAccumulator {
    pub fn new(column_name: String) -> Self {
        Self {
            column_name, 
            count: 0,
        }
    }
}

impl Accumulator for CountAccumulator {
    fn update(&mut self, batch: &RecordBatch) -> Result<(), ExecutionError> {
        self.count += batch.num_rows() as u64;
        Ok(())
    }

    fn finalize(&mut self) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![self.count]))
    }   

    fn output_field(&self) -> Field {
        Field::new(
            format!("count({})", self.column_name), 
            DataType::UInt64,
            false
        )
    }
}