//! Aggregation module.
//!
//! Each aggregate is implemented in two layers:
//!
//! 1. A generic math struct (e.g. `Sum<T>`) with static init/update/merge/finalize
//!     methods.  merge() method is for parallel implementations,
//!     and not yet integrated.
//!
//!  2. A runtime wrapper (e.g. `SumAgg`) that holds a typed state enum and
//!     implements the `Aggregator` trait. Query executor uses this wrapper.
//!


pub mod sum;
pub mod max;
pub mod min;
pub mod avg;
pub mod count;
pub mod top_k;
pub mod factory;

use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

/// A stateful, runtime-dispatched aggregate.
///
/// One instance per aggregate expression in a query. The operator feeds it
/// `ColumnChunk`s via `update`, then calls `finalize` exactly once to obtain
/// a single-row result column.
///
/// Implementors live alongside the math they wrap (e.g. `SumAgg` next to
/// `Sum<T>` in `sum.rs`). The math types stay generic over `T`; the wrapper
/// owns the runtime type dispatch.
pub trait Aggregator {
    /// Fold one batch of input values into the running state.
    /// Returns `InvalidData` if `chunk`'s type is not what this aggregate
    /// was built for (a bug — the analyser should have caught it).
    fn update(&mut self, chunk: &ColumnChunk) -> Result<(), ExecutionError>;

    /// Consume the aggregator and produce its single-row result column.
    fn finalize(&mut self) -> ColumnChunk;

    /// The `DataType` of the column produced by `finalize`. Used by the
    /// operator to build the output schema before draining the input.
    fn output_type(&self) -> DataType;
}

pub use factory::build;
