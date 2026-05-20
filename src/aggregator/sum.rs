//! Sum aggregate over numeric types.
//!
//! State and output share the input type T. Overflow uses default `+`
//! (panic in debug, wrap in release). A future enhancement could promote
//! narrow integer inputs to a wider accumulator.

use std::marker::PhantomData;
use std::ops::AddAssign;

pub struct Sum<T>(PhantomData<T>);

impl<T> Sum<T>
where
    T: Copy + Default + AddAssign + std::iter::Sum,
{
    pub fn init() -> T {
        T::default()
    }

    pub fn update(state: &mut T, input: &[T]) {
        *state += input.iter().copied().sum::<T>();
    }

    pub fn merge(a: &mut T, b: T) {
        *a += b;
    }

    pub fn finalize(state: T) -> T {
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_i64_single_chunk() {
        let mut state = Sum::<i64>::init();
        Sum::<i64>::update(&mut state, &[1, 2, 3, 4]);
        assert_eq!(Sum::<i64>::finalize(state), 10);
    }

    #[test]
    fn sum_i32_single_chunk() {
        let mut state = Sum::<i32>::init();
        Sum::<i32>::update(&mut state, &[1, 2, 3, 4]);
        assert_eq!(Sum::<i32>::finalize(state), 10);
    }

    #[test]
    fn sum_empty_is_zero() {
        let mut state = Sum::<i64>::init();
        Sum::<i64>::update(&mut state, &[]);
        assert_eq!(Sum::<i64>::finalize(state), 0);
    }

    #[test]
    fn sum_i64_merge_matches_full() {
        let data: Vec<i64> = (1..=100).collect();

        let full = {
            let mut s = Sum::<i64>::init();
            Sum::<i64>::update(&mut s, &data);
            Sum::<i64>::finalize(s)
        };

        for split in [0, 1, 50, 99, 100] {
            let mut left = Sum::<i64>::init();
            Sum::<i64>::update(&mut left, &data[..split]);
            let mut right = Sum::<i64>::init();
            Sum::<i64>::update(&mut right, &data[split..]);
            Sum::<i64>::merge(&mut left, right);
            assert_eq!(Sum::<i64>::finalize(left), full, "split {split}");
        }
    }

    #[test]
    fn sum_i32_merge_matches_full() {
        let data: Vec<i32> = (1..=100).collect();

        let full = {
            let mut s = Sum::<i32>::init();
            Sum::<i32>::update(&mut s, &data);
            Sum::<i32>::finalize(s)
        };

        for split in [0, 1, 50, 99, 100] {
            let mut left = Sum::<i32>::init();
            Sum::<i32>::update(&mut left, &data[..split]);
            let mut right = Sum::<i32>::init();
            Sum::<i32>::update(&mut right, &data[split..]);
            Sum::<i32>::merge(&mut left, right);
            assert_eq!(Sum::<i32>::finalize(left), full, "split {split}");
        }
    }
}

use crate::aggregator::Aggregator;
use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

/// Runtime-dispatched wrapper around `Sum<T>`.
///
/// Holds a typed state matching the input column's `DataType`. Each call to
/// `update` matches both state and chunk to delegate to the right `Sum<T>`.
/// Output type equals input type (no widening yet).
enum SumState {
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    U64(Vec<u64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

pub struct SumAgg {
    state: SumState,
}

impl SumAgg {
    pub fn new(input: DataType) -> Result<Self, ExecutionError> {
        let state = match input {
            DataType::I8 => SumState::I8(vec![]),
            DataType::I16 => SumState::I16(vec![]),
            DataType::I32 => SumState::I32(vec![]),
            DataType::I64 => SumState::I64(vec![]),
            DataType::U8 => SumState::U8(vec![]),
            DataType::U16 => SumState::U16(vec![]),
            DataType::U32 => SumState::U32(vec![]),
            DataType::U64 => SumState::U64(vec![]),
            DataType::F32 => SumState::F32(vec![]),
            DataType::F64 => SumState::F64(vec![]),
            other => {
                return Err(ExecutionError::InvalidData(format!(
                    "SUM is not supported for type {:?}",
                    other
                )));
            }
        };
        Ok(Self { state })
    }
}

impl Aggregator for SumAgg {
    fn update(
        &mut self,
        chunk: &ColumnChunk,
        group_ids: &[u32],
        n_groups: usize,
    ) -> Result<(), ExecutionError> {
        match (&mut self.state, chunk) {
            (SumState::I8(acc), ColumnChunk::I8(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::I16(acc), ColumnChunk::I16(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::I32(acc), ColumnChunk::I32(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::I64(acc), ColumnChunk::I64(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::U8(acc), ColumnChunk::U8(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::U16(acc), ColumnChunk::U16(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::U32(acc), ColumnChunk::U32(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::U64(acc), ColumnChunk::U64(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::F32(acc), ColumnChunk::F32(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0.0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            (SumState::F64(acc), ColumnChunk::F64(vals)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, 0.0);
                }
                for (&val, &gid) in vals.iter().zip(group_ids) {
                    acc[gid as usize] += val;
                }
            }
            _ => {
                return Err(ExecutionError::InvalidData(
                    "SUM: state/chunk type mismatch (planner bug)".into(),
                ));
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        match &self.state {
            SumState::I8(acc) => ColumnChunk::I8(acc.clone()),
            SumState::I16(acc) => ColumnChunk::I16(acc.clone()),
            SumState::I32(acc) => ColumnChunk::I32(acc.clone()),
            SumState::I64(acc) => ColumnChunk::I64(acc.clone()),
            SumState::U8(acc) => ColumnChunk::U8(acc.clone()),
            SumState::U16(acc) => ColumnChunk::U16(acc.clone()),
            SumState::U32(acc) => ColumnChunk::U32(acc.clone()),
            SumState::U64(acc) => ColumnChunk::U64(acc.clone()),
            SumState::F32(acc) => ColumnChunk::F32(acc.clone()),
            SumState::F64(acc) => ColumnChunk::F64(acc.clone()),
        }
    }

    fn output_type(&self) -> DataType {
        match self.state {
            SumState::I8(_) => DataType::I8,
            SumState::I16(_) => DataType::I16,
            SumState::I32(_) => DataType::I32,
            SumState::I64(_) => DataType::I64,
            SumState::U8(_) => DataType::U8,
            SumState::U16(_) => DataType::U16,
            SumState::U32(_) => DataType::U32,
            SumState::U64(_) => DataType::U64,
            SumState::F32(_) => DataType::F32,
            SumState::F64(_) => DataType::F64,
        }
    }
}
