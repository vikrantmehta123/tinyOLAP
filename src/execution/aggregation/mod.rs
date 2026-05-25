pub mod count;
pub mod hash_aggregate;
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
pub trait Accumulator {
    // Update the Accumulator's state with this batch
    fn update(&mut self, batch: &RecordBatch) -> Result<(), ExecutionError>;

    /// Produce the final ArrayRef from the accumulated state.
    fn finalize(&mut self) -> ArrayRef;

    /// Describe my output column for schema construction.
    fn output_field(&self) -> Field;
}
