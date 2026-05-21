//! Min aggregate over integer and float types.
//!
//! Integer state uses `Min<T>` (`Ord`). Float state uses `MinFloat<T>`
//! (`PartialOrd`, NaN-skipping — same convention as `MaxFloat`).
//! State is `Option<T>`: `None` means no values seen yet.
//!
//! Two structs exist for Min: for ints and floats. Similar to Max

use std::marker::PhantomData;

use crate::aggregator::Aggregator;
use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

pub struct Min<T>(PhantomData<T>);

impl<T> Min<T>
where
    T: Copy + Ord,
{
    pub fn init() -> Option<T> {
        None
    }

    pub fn update(state: &mut Option<T>, input: &[T]) {
        let chunk_min = input.iter().copied().min();
        Self::merge(state, chunk_min);
    }

    pub fn merge(a: &mut Option<T>, b: Option<T>) {
        match (*a, b) {
            (_, None) => {}
            (None, Some(bv)) => *a = Some(bv),
            (Some(av), Some(bv)) => *a = Some(av.min(bv)),
        }
    }

    pub fn finalize(state: Option<T>) -> Option<T> {
        state
    }
}

// NaN handling: comparisons against NaN return false in both directions,
// so NaNs are never adopted as the min and never replace the current min.
pub struct MinFloat<T>(PhantomData<T>);

impl<T> MinFloat<T>
where
    T: Copy + PartialOrd,
{
    pub fn init() -> Option<T> {
        None
    }

    pub fn update(state: &mut Option<T>, input: &[T]) {
        for &v in input {
            match *state {
                None => *state = Some(v),
                Some(current) if v < current => *state = Some(v),
                _ => {}
            }
        }
    }

    pub fn merge(a: &mut Option<T>, b: Option<T>) {
        match (*a, b) {
            (_, None) => {}
            (None, Some(bv)) => *a = Some(bv),
            (Some(av), Some(bv)) if bv < av => *a = Some(bv),
            _ => {}
        }
    }

    pub fn finalize(state: Option<T>) -> Option<T> {
        state
    }
}

enum MinState {
    I8(Vec<Option<i8>>),
    I16(Vec<Option<i16>>),
    I32(Vec<Option<i32>>),
    I64(Vec<Option<i64>>),
    U8(Vec<Option<u8>>),
    U16(Vec<Option<u16>>),
    U32(Vec<Option<u32>>),
    U64(Vec<Option<u64>>),
    F32(Vec<Option<f32>>),
    F64(Vec<Option<f64>>),
}

pub struct MinAgg {
    state: MinState,
}

impl MinAgg {
    pub fn new(input: DataType) -> Result<Self, ExecutionError> {
        let state = match input {
            DataType::I8 => MinState::I8(vec![]),
            DataType::I16 => MinState::I16(vec![]),
            DataType::I32 => MinState::I32(vec![]),
            DataType::I64 => MinState::I64(vec![]),
            DataType::U8 => MinState::U8(vec![]),
            DataType::U16 => MinState::U16(vec![]),
            DataType::U32 => MinState::U32(vec![]),
            DataType::U64 => MinState::U64(vec![]),
            DataType::F32 => MinState::F32(vec![]),
            DataType::F64 => MinState::F64(vec![]),
            other => {
                return Err(ExecutionError::InvalidData(format!(
                    "MIN is not supported for type {:?}",
                    other
                )));
            }
        };
        Ok(Self { state })
    }
}

impl Aggregator for MinAgg {
    fn update(
        &mut self,
        chunk: &ColumnChunk,
        group_ids: &[u32],
        n_groups: usize,
    ) -> Result<(), ExecutionError> {
        match (&mut self.state, chunk) {
            (MinState::I8(acc), ColumnChunk::I8(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<i8>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::I16(acc), ColumnChunk::I16(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<i16>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::I32(acc), ColumnChunk::I32(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<i32>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::I64(acc), ColumnChunk::I64(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<i64>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::U8(acc), ColumnChunk::U8(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<u8>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::U16(acc), ColumnChunk::U16(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<u16>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::U32(acc), ColumnChunk::U32(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<u32>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::U64(acc), ColumnChunk::U64(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    Min::<u64>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::F32(acc), ColumnChunk::F32(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    MinFloat::<f32>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            (MinState::F64(acc), ColumnChunk::F64(v)) => {
                if acc.len() < n_groups {
                    acc.resize(n_groups, None);
                }
                for (&val, &gid) in v.iter().zip(group_ids) {
                    MinFloat::<f64>::merge(&mut acc[gid as usize], Some(val));
                }
            }
            _ => {
                return Err(ExecutionError::InvalidData(
                    "MIN: state/chunk type mismatch (planner bug)".into(),
                ));
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        match &self.state {
            MinState::I8(acc) => {
                ColumnChunk::I8(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::I16(acc) => {
                ColumnChunk::I16(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::I32(acc) => {
                ColumnChunk::I32(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::I64(acc) => {
                ColumnChunk::I64(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::U8(acc) => {
                ColumnChunk::U8(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::U16(acc) => {
                ColumnChunk::U16(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::U32(acc) => {
                ColumnChunk::U32(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::U64(acc) => {
                ColumnChunk::U64(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::F32(acc) => {
                ColumnChunk::F32(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
            MinState::F64(acc) => {
                ColumnChunk::F64(acc.iter().map(|s| s.unwrap_or_default()).collect())
            }
        }
    }

    fn output_type(&self) -> DataType {
        match self.state {
            MinState::I8(_) => DataType::I8,
            MinState::I16(_) => DataType::I16,
            MinState::I32(_) => DataType::I32,
            MinState::I64(_) => DataType::I64,
            MinState::U8(_) => DataType::U8,
            MinState::U16(_) => DataType::U16,
            MinState::U32(_) => DataType::U32,
            MinState::U64(_) => DataType::U64,
            MinState::F32(_) => DataType::F32,
            MinState::F64(_) => DataType::F64,
        }
    }
}
