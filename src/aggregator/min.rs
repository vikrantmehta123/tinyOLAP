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
    pub fn init() -> Option<T> { None }

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

    pub fn finalize(state: Option<T>) -> Option<T> { state }
}

// NaN handling: comparisons against NaN return false in both directions,
// so NaNs are never adopted as the min and never replace the current min.
pub struct MinFloat<T>(PhantomData<T>);

impl<T> MinFloat<T>
where
    T: Copy + PartialOrd,
{
    pub fn init() -> Option<T> { None }

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

    pub fn finalize(state: Option<T>) -> Option<T> { state }
}

enum MinState {
    I8(Option<i8>),   I16(Option<i16>), I32(Option<i32>), I64(Option<i64>),
    U8(Option<u8>),   U16(Option<u16>), U32(Option<u32>), U64(Option<u64>),
    F32(Option<f32>), F64(Option<f64>),
}

pub struct MinAgg {
    state: MinState,
}

impl MinAgg {
    pub fn new(input: DataType) -> Result<Self, ExecutionError> {
        let state = match input {
            DataType::I8  => MinState::I8(Min::<i8>::init()),
            DataType::I16 => MinState::I16(Min::<i16>::init()),
            DataType::I32 => MinState::I32(Min::<i32>::init()),
            DataType::I64 => MinState::I64(Min::<i64>::init()),
            DataType::U8  => MinState::U8(Min::<u8>::init()),
            DataType::U16 => MinState::U16(Min::<u16>::init()),
            DataType::U32 => MinState::U32(Min::<u32>::init()),
            DataType::U64 => MinState::U64(Min::<u64>::init()),
            DataType::F32 => MinState::F32(MinFloat::<f32>::init()),
            DataType::F64 => MinState::F64(MinFloat::<f64>::init()),
            other => return Err(ExecutionError::InvalidData(
                format!("MIN is not supported for type {:?}", other)
            )),
        };
        Ok(Self { state })
    }
}

impl Aggregator for MinAgg {
    fn update(&mut self, chunk: &ColumnChunk) -> Result<(), ExecutionError> {
        match (&mut self.state, chunk) {
            (MinState::I8(s),  ColumnChunk::I8(v))  => Min::<i8>::update(s, v),
            (MinState::I16(s), ColumnChunk::I16(v)) => Min::<i16>::update(s, v),
            (MinState::I32(s), ColumnChunk::I32(v)) => Min::<i32>::update(s, v),
            (MinState::I64(s), ColumnChunk::I64(v)) => Min::<i64>::update(s, v),
            (MinState::U8(s),  ColumnChunk::U8(v))  => Min::<u8>::update(s, v),
            (MinState::U16(s), ColumnChunk::U16(v)) => Min::<u16>::update(s, v),
            (MinState::U32(s), ColumnChunk::U32(v)) => Min::<u32>::update(s, v),
            (MinState::U64(s), ColumnChunk::U64(v)) => Min::<u64>::update(s, v),
            (MinState::F32(s), ColumnChunk::F32(v)) => MinFloat::<f32>::update(s, v),
            (MinState::F64(s), ColumnChunk::F64(v)) => MinFloat::<f64>::update(s, v),
            _ => return Err(ExecutionError::InvalidData(
                "MIN: state/chunk type mismatch (planner bug)".into()
            )),
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        match self.state {
            MinState::I8(s)  => ColumnChunk::I8(vec![Min::<i8>::finalize(s).unwrap_or_default()]),
            MinState::I16(s) => ColumnChunk::I16(vec![Min::<i16>::finalize(s).unwrap_or_default()]),
            MinState::I32(s) => ColumnChunk::I32(vec![Min::<i32>::finalize(s).unwrap_or_default()]),
            MinState::I64(s) => ColumnChunk::I64(vec![Min::<i64>::finalize(s).unwrap_or_default()]),
            MinState::U8(s)  => ColumnChunk::U8(vec![Min::<u8>::finalize(s).unwrap_or_default()]),
            MinState::U16(s) => ColumnChunk::U16(vec![Min::<u16>::finalize(s).unwrap_or_default()]),
            MinState::U32(s) => ColumnChunk::U32(vec![Min::<u32>::finalize(s).unwrap_or_default()]),
            MinState::U64(s) => ColumnChunk::U64(vec![Min::<u64>::finalize(s).unwrap_or_default()]),
            MinState::F32(s) => ColumnChunk::F32(vec![MinFloat::<f32>::finalize(s).unwrap_or_default()]),
            MinState::F64(s) => ColumnChunk::F64(vec![MinFloat::<f64>::finalize(s).unwrap_or_default()]),
        }
    }

    fn output_type(&self) -> DataType {
        match self.state {
            MinState::I8(_)  => DataType::I8,
            MinState::I16(_) => DataType::I16,
            MinState::I32(_) => DataType::I32,
            MinState::I64(_) => DataType::I64,
            MinState::U8(_)  => DataType::U8,
            MinState::U16(_) => DataType::U16,
            MinState::U32(_) => DataType::U32,
            MinState::U64(_) => DataType::U64,
            MinState::F32(_) => DataType::F32,
            MinState::F64(_) => DataType::F64,
        }
    }
}
