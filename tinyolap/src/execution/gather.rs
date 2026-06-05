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
use crossbeam_channel::Receiver;

use crate::execution::executor::{ExecutionError, ExecutionPlan};
use std::{fmt, thread::JoinHandle};

pub struct GatherExec {
    n_inputs: usize, 
    rx: Option<Receiver<Result<RecordBatch, ExecutionError>>>,
    handles: Vec<Option<JoinHandle<()>>>,
    child_display: String,
}

impl GatherExec {
    pub fn new(n_inputs:usize, children: Vec<Box<dyn ExecutionPlan>>) -> Self {
        let child_display = format!("{}", children[0]);

        let (tx, rx) = crossbeam_channel::bounded(128);
        let mut handles = Vec::with_capacity(n_inputs);

        for child in children {
            let tx = tx.clone();
            let handle = std::thread::spawn(move || {
                let mut child = child;
                loop {
                    
                    match child.next_batch() {
                        Some(batch_result) => {
                            if tx.send(batch_result).is_err() {
                                break;
                            }
                        },
                        None => break,
                    }
                }
            });
            handles.push(Some(handle));
        }

        Self {
            n_inputs, 
            handles,
            rx: Some(rx),
            child_display
        }
    }
}

impl ExecutionPlan for GatherExec {
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        
        match self.rx.as_ref().unwrap().recv() {
            Ok(res) => Some(res), 
            Err(_) => None
        }
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Gather(workers={})", indent, self.n_inputs)?;
        let child_shift = "  ".repeat(depth + 1);
        for line in self.child_display.lines() {
            writeln!(f, "{}{}", child_shift, line)?;
        }
        Ok(())
    }
}
 
/// Pretty Print the operator
impl fmt::Display for GatherExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

/// TODO: Understand why this is required.
impl Drop for GatherExec {
    fn drop(&mut self) {
        drop(self.rx.take());

        for slot in &mut self.handles {
            // TODO: worker panics are swallowed here — consumer sees clean end-of-stream
            // instead of an error. Route panics through the channel as ExecutionError later.
            if let Some(handle) = slot.take() {
                let _ = handle.join();
            }
        }

    }
}
