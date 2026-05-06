//! Filter Processor
//! 
//! From a given batch, evaluate the predicate and select only
//! the rows that pass the predicate. 
//! 
//! Evaluator produces a boolean mask. Filter uses that mask to copy
//! the matching rows into a new chunk.

use crate::evaluator::evaluate;
use crate::parser::ast::Predicate;

use super::{
    batch::Batch,
    processor::{ExecutionError, Processor},
};

pub struct Filter {
    input: Box<dyn Processor>,
    predicate: Predicate,
}

impl Filter {
    pub fn new(input: Box<dyn Processor>, predicate: Predicate) -> Self {
        Self { input, predicate }
    }
}

impl Processor for Filter {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>> {
        let batch = match self.input.next_batch()? {
            Ok(b) => b,
            Err(e) => return Some(Err(e)),
        };

        let mask = {
            let named: Vec<(&str, &_)> = batch
                .schema
                .iter()
                .map(|c| c.name.as_str())
                .zip(batch.columns.iter())
                .collect();
            match evaluate(&self.predicate, &named) {
                Ok(m) => m,
                Err(e) => return Some(Err(ExecutionError::InvalidData(e.to_string()))),
            }
        };

        let columns = batch.columns.iter().map(|c| c.filter(&mask)).collect();

        Some(Ok(Batch { schema: batch.schema, columns }))
    }
}
