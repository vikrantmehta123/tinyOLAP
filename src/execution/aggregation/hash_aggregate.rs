use std::sync::Arc;
use std::fmt;

use arrow::{array::{ArrayRef, RecordBatch}, datatypes::Schema};

use crate::execution::{aggregation::Accumulator, executor::{ExecutionError, ExecutionPlan}};

pub struct HashAggregateExec {
    child: Box<dyn ExecutionPlan>, 
    output_schema: Arc<Schema>,
    emitted: bool, 
    accumulators: Vec<Box<dyn Accumulator>>,
}


impl HashAggregateExec {
    pub fn new(accumulators: Vec<Box<dyn Accumulator>>, child: Box<dyn ExecutionPlan>) -> Self {
         
         let fields: Vec<_> = accumulators.iter().map(|a| a.output_field()).collect();
         let output_schema = Arc::new(Schema::new(fields));

         Self {
            child, 
            accumulators, 
            output_schema, 
            emitted: false,
         }
    }
}

impl fmt::Display for HashAggregateExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl ExecutionPlan for HashAggregateExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {

        if self.emitted {
            return None;
        }

        loop {
            let batch = match self.child.next_batch() {
                None             => break,
                Some(Ok(b))      => b,
                Some(Err(e))     => return Some(Err(e)),
            }; 

            for acc in self.accumulators.iter_mut() {
                if let Err(e) = acc.update(&batch) {
                    return Some(Err(e));
                }
            }
        }

        // Drain finished — ask every accumulator for its output column.
        let arrays: Vec<ArrayRef> = self.accumulators.iter_mut()
            .map(|acc| acc.finalize())
            .collect();

        let batch = match RecordBatch::try_new(self.output_schema.clone(), arrays) {
            Ok(b) => b, 
            Err(e) => return Some(Err(e.into())),
        };

        self.emitted = true;
        Some(Ok(batch))
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}HashAggregate(n_aggs={})", indent, self.accumulators.len())?;
        self.child.fmt_indented(f, depth + 1)
    }
}