use crate::aggregator::Aggregator;
use crate::storage::schema::ColumnDef;

use super::{
    batch::Batch,
    processor::{ExecutionError, Processor},
};

pub struct Aggregate {
    input: Box<dyn Processor>,
    aggs: Vec<Box<dyn Aggregator>>,
    input_idx: Vec<usize>,
    output_schema: Vec<ColumnDef>,
    done: bool,
}

impl Aggregate {
    pub fn new(
        input: Box<dyn Processor>,
        aggs: Vec<Box<dyn Aggregator>>,
        input_idx: Vec<usize>,
        output_schema: Vec<ColumnDef>,
    ) -> Self {
        Self {
            input,
            aggs,
            input_idx,
            output_schema,
            done: false,
        }
    }
}

impl Processor for Aggregate {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>> {
        if self.done {
            return None;
        }

        // Drain phase: pull every batch from the input and fold each
        // aggregate's input column into its state.
        while let Some(result) = self.input.next_batch() {
            let batch = match result {
                Ok(b) => b,
                Err(e) => return Some(Err(e)),
            };
            for (agg, &idx) in self.aggs.iter_mut().zip(self.input_idx.iter()) {
                let group_ids = vec![0u32; batch.columns[idx].len()];
                if let Err(e) = agg.update(&batch.columns[idx], &group_ids, 1) {
                    return Some(Err(e));
                }
            }
        }

        // Emit phase: ask each aggregator for its 1-row column.
        let columns = self.aggs.iter_mut().map(|agg| agg.finalize()).collect();

        self.done = true;

        Some(Ok(Batch {
            schema: self.output_schema.clone(),
            columns,
        }))
    }
}
