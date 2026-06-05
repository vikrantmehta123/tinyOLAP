//! Relational Algebra Operator: Projection
//! 
//! Selects the columns that we want in the output.

use arrow::array::RecordBatch;
use std::fmt;
use crate::execution::executor::{ExecutionError, ExecutionPlan};
use crate::physical_plan::physical_operators::PhysicalExpr;

pub struct ProjectExec {
    projections: Vec<PhysicalExpr>, // The columns to select
    child: Box<dyn ExecutionPlan>,
}

impl ProjectExec {
    pub fn new(projections: Vec<PhysicalExpr>, child: Box<dyn ExecutionPlan>) -> Self {
        Self { projections, child }
    }
}

impl ExecutionPlan for ProjectExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        let batch = match self.child.next_batch()? {
            Ok(b) => b,
            Err(e) => return Some(Err(e)),
        };

        // Resolve each projection (must be a Column ref) to an index in the
        // input batch's schema. Anything else is a planner bug.
        let indices: Result<Vec<usize>, ExecutionError> = self
            .projections
            .iter()
            .map(|expr| match expr {
                PhysicalExpr::Column(name) => {
                    batch.schema().index_of(name).map_err(ExecutionError::from)
                }
                _ => Err(ExecutionError::InvalidData(
                    "ProjectExec supports only bare column references".into(),
                )),
            })
            .collect();

        let indices = match indices {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };

        // Zero-copy column selection: the new batch shares the same ArrayRefs
        // as the input, just with a new column order.
        match batch.project(&indices) {
            Ok(b) => Some(Ok(b)),
            Err(e) => Some(Err(e.into())),
        }
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        let cols: Vec<String> = self.projections.iter().map(|e| e.to_string()).collect();
        writeln!(f, "{}Project([{}])", indent, cols.join(", "))?;
        self.child.fmt_indented(f, depth + 1)
    }
}

impl fmt::Display for ProjectExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
