use arrow::array::RecordBatch;
use std::fmt;

use crate::execution::executor::{ExecutionError, ExecutionPlan};

pub struct LimitExec {
    limit: u64,
    emitted: u64,
    child: Box<dyn ExecutionPlan>,
}

impl LimitExec {
    pub fn new(limit: u64, child: Box<dyn ExecutionPlan>) -> Self {
        Self {
            limit: limit,
            emitted: 0,
            child: child,
        }
    }
}

impl ExecutionPlan for LimitExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        if self.emitted >= self.limit {
            return None;
        }

        let batch = match self.child.next_batch()? {
            Ok(b) => b,
            Err(e) => return Some(Err(e)),
        };

        let remaining = self.limit - self.emitted;
        let rows = batch.num_rows() as u64;

        if rows <= remaining {
            // Whole batch fits under the limit — pass through.
            self.emitted += rows;
            Some(Ok(batch))
        } else {
            // This batch crosses the limit — slice to exactly what we need.
            // RecordBatch::slice is zero-copy; the slice shares the same
            // ArrayRefs as the original, just with a different length.
            self.emitted = self.limit;
            Some(Ok(batch.slice(0, remaining as usize)))
        }
    }
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Limit({})", indent, self.limit)?;
        self.child.fmt_indented(f, depth + 1)
    }
}

impl fmt::Display for LimitExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
