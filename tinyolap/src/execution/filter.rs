//! FilterExec: Filters rows from batch based on predicate
//!
//! Given a predicate and a batch of rows, we want to evaluate
//! which rows pass the predicate and emit those rows as a new batch.
//! FilterExec implements this.

use arrow::array::RecordBatch;
use arrow::compute::filter_record_batch;
use std::boxed::Box;
use std::fmt;

use crate::execution::executor::{ExecutionError, ExecutionPlan};
use crate::execution::expr::evaluate_predicate;
use crate::physical_plan::physical_operators::PhysicalExpr;

pub struct FilterExec {
    predicate: PhysicalExpr,
    child: Box<dyn ExecutionPlan>,
}

impl FilterExec {
    pub fn new(predicate: PhysicalExpr, child: Box<dyn ExecutionPlan>) -> Self {
        Self { predicate, child }
    }
}

impl ExecutionPlan for FilterExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        loop {
            let batch = match self.child.next_batch()? {
                Ok(b) => b,
                Err(e) => return Some(Err(e)),
            };

            // Do two tight loops: first compute the mask and then filter the rows
            let mask = match evaluate_predicate(&self.predicate, &batch) {
                Ok(m) => m,
                Err(e) => return Some(Err(e)),
            };

            let filtered = match filter_record_batch(&batch, &mask) {
                Ok(b) => b,
                Err(e) => return Some(Err(e.into())),
            };

            // Skip empty batches — keep the non-empty contract for downstream.
            // Aggregations treat empty batch differently. So don't return
            // anything if there's an empty batch
            if filtered.num_rows() > 0 {
                return Some(Ok(filtered));
            }
            // else: this batch filtered to zero rows, pull the next one
        }
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Filter({})", indent, self.predicate)?;
        self.child.fmt_indented(f, depth + 1)
    }
}

impl fmt::Display for FilterExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
