//! This module defines the traits and structs for aggregations
//! 
//! We support sum, avg, count, min, max. Each aggregate function
//! implements an Accumulator trait. This trait knows how to update
//! the internal state of the aggregate and how to merge and finalise
//! the outputs

pub mod avg;
pub mod count;
pub mod hash_aggregate;
pub mod max;
pub mod merge_aggregate;
pub mod min;
pub mod sum;

use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::Field,
};

use crate::execution::executor::ExecutionError;

/// Per-aggregate state and behavior. One implementor per aggregate function
/// (SumAccumulator<T>, CountAccumulator, AvgAccumulator, etc.).
/// HashAggregateExec drives a Vec<Box<dyn Accumulator>> — feeds each batch
/// to every accumulator via update(), then collects finalize() results.
pub trait Accumulator: Send {
    // Update the Accumulator's state with this batch
    fn update(
        &mut self,
        batch: &RecordBatch,
        group_indices: &[u32],
        num_groups: usize,
    ) -> Result<(), ExecutionError>;

    // Used by MergeAggregateExec
    // N accumulators will run in parallel in HashAggregateExec
    // Due to this, we need to introduce a merge operation to appropriately
    // combine the results from different threads.
    // This is done in MergeAggregateExec
    fn merge(
        &mut self,
        batch: &RecordBatch,
        group_indices: &[u32],
        num_groups: usize,
    ) -> Result<(), ExecutionError>;

    /// Produce the final ArrayRef from the accumulated intermediate state.
    fn materialize(&mut self) -> ArrayRef;

    /// Describe my output column for schema construction.
    fn output_field(&self) -> Field;

    // If GROUP BY clause, then we let GROUP by code drive the capacity resizing.
    // If no GROUP BY clause and if empty table, then we need to emit one row with default value.
    // This function is to handle this edge case.
    fn ensure_capacity(&mut self, num_groups: usize);
}
