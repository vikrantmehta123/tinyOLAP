//! GatherExec operator: N inputs → 1 output. Combines streams.
//! 
//! In the current implementation of tinyOLAP, the GatherExec operator will always 
//! be the last operator. Because currently, tinyOLAP doesn't support JOINs, nor SORT. 
//! As a result, there is only a single pipeline that it has to execute and it can be
//! parallelized at QueryPlanning time.
//! 
//! The Builder module will fan-out copies of the plan for each thread. Each thread
//! can execute parallely and at the end, GatherExec will collect those results and 
//! produce outputs from the query. Because of current scope and limitations, 
//! there is no ScatterExec operator as well. Builder itself fans-out.
//! 
//! If we decide to add SORT/JOIN, the implementation of GatherExec can be used.
//! GatherExec is not tied to being the root node- it just so happens that it will
//! always be the root node in the current implementation of tinyOLAP.

use arrow::array::RecordBatch;

use crate::execution::executor::{ExecutionError, ExecutionPlan};
use std::fmt;

pub struct GatherExec {
    n_inputs: usize, 
    child: Box<dyn ExecutionPlan>,
}

impl GatherExec {
    pub fn new(n_inputs:usize, child: Box<dyn ExecutionPlan>) -> Self {
        Self {
            n_inputs, 
            child
        }
    }
}

impl ExecutionPlan for GatherExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        
        self.child.next_batch()
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Gather(workers={})", indent, self.n_inputs)?;
        self.child.fmt_indented(f, depth + 1)
    }
}

/// Pretty Print the operator
impl fmt::Display for GatherExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}
