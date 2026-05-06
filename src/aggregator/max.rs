//! Max aggregate over integer types.
//!
//! State is `Option<T>`: `None` means no values seen yet (matches SQL's
//! "MAX over zero rows is NULL" behavior).
//!
//! Bound is `Ord` (total order), which admits all integer types but not
//! floats — floats only implement `PartialOrd` because of NaN.
//! 
//! Two implementations exist for Max: one for floats and one for ints.

use std::marker::PhantomData;

pub struct Max<T>(PhantomData<T>);

impl<T> Max<T>
where
    T: Copy + Ord,
{
    pub fn init() -> Option<T> {
        None
    }

    pub fn update(state: &mut Option<T>, input: &[T]) {
        let chunk_max = input.iter().copied().max();
        Self::merge(state, chunk_max);
    }

    pub fn merge(a: &mut Option<T>, b: Option<T>) {
        match (*a, b) {
            (_, None) => {}
            (None, Some(bv)) => *a = Some(bv),
            (Some(av), Some(bv)) => *a = Some(av.max(bv)),
        }
    }

    pub fn finalize(state: Option<T>) -> Option<T> {
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_i64_basic() {
        let mut s = Max::<i64>::init();
        Max::<i64>::update(&mut s, &[3, 1, 4, 1, 5, 9, 2, 6]);
        assert_eq!(Max::<i64>::finalize(s), Some(9));
    }

    #[test]
    fn max_i32_negatives() {
        let mut s = Max::<i32>::init();
        Max::<i32>::update(&mut s, &[-10, -3, -7, -1, -50]);
        assert_eq!(Max::<i32>::finalize(s), Some(-1));
    }

    #[test]
    fn max_u8() {
        let mut s = Max::<u8>::init();
        Max::<u8>::update(&mut s, &[10, 200, 50]);
        assert_eq!(Max::<u8>::finalize(s), Some(200));
    }

    #[test]
    fn max_empty_is_none() {
        let mut s = Max::<i64>::init();
        Max::<i64>::update(&mut s, &[]);
        assert_eq!(Max::<i64>::finalize(s), None);
    }

    #[test]
    fn max_merge_matches_full() {
        let data: Vec<i64> = vec![5, 3, 8, 1, 9, 2, 7, 4, 6, 0];

        let full = {
            let mut s = Max::<i64>::init();
            Max::<i64>::update(&mut s, &data);
            Max::<i64>::finalize(s)
        };

        for split in [0, 1, 5, 9, 10] {
            let mut left = Max::<i64>::init();
            Max::<i64>::update(&mut left, &data[..split]);
            let mut right = Max::<i64>::init();
            Max::<i64>::update(&mut right, &data[split..]);
            Max::<i64>::merge(&mut left, right);
            assert_eq!(Max::<i64>::finalize(left), full, "split {split}");
        }
    }

    #[test]
    fn max_merge_with_empty_partial() {
        let mut left = Max::<i32>::init();
        Max::<i32>::update(&mut left, &[1, 2, 3]);
        let right = Max::<i32>::init(); // stays None
        Max::<i32>::merge(&mut left, right);
        assert_eq!(Max::<i32>::finalize(left), Some(3));
    }
}

// Max aggregate over float types.
//
// NaN handling: comparisons against NaN return false in both directions,
// so NaN values are never adopted as the max and never replace the
// current max. Effectively, NaNs are skipped. This matches most databases.

pub struct MaxFloat<T>(PhantomData<T>);

impl<T> MaxFloat<T>
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
                Some(current) if v > current => *state = Some(v),
                _ => {}
            }
        }
    }

    pub fn merge(a: &mut Option<T>, b: Option<T>) {
        match (*a, b) {
            (_, None) => {}
            (None, Some(bv)) => *a = Some(bv),
            (Some(av), Some(bv)) if bv > av => *a = Some(bv),
            _ => {}
        }
    }

    pub fn finalize(state: Option<T>) -> Option<T> {
        state
    }
}


use crate::aggregator::Aggregator;
use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

/// Runtime-dispatched wrapper around `Max<T>` (integers) and `MaxFloat<T>` (floats).
///
/// Integer state uses `Max<T>` (requires `Ord`). Float state uses `MaxFloat<T>`
/// (`PartialOrd`, NaN-skipping). Output type equals input type.
///
/// Empty-input semantics: SQL says MAX over zero rows is NULL, but `ColumnChunk`
/// doesn't model nulls yet, so `finalize` returns `T::default()` (0 / 0.0) on
/// `None`. Revisit when nulls land.
enum MaxState {
    I8(Option<i8>),   I16(Option<i16>), I32(Option<i32>), I64(Option<i64>),
    U8(Option<u8>),   U16(Option<u16>), U32(Option<u32>), U64(Option<u64>),
    F32(Option<f32>), F64(Option<f64>),
}

pub struct MaxAgg {
    state: MaxState,
}

impl MaxAgg {
    pub fn new(input: DataType) -> Result<Self, ExecutionError> {
        let state = match input {
            DataType::I8  => MaxState::I8(Max::<i8>::init()),
            DataType::I16 => MaxState::I16(Max::<i16>::init()),
            DataType::I32 => MaxState::I32(Max::<i32>::init()),
            DataType::I64 => MaxState::I64(Max::<i64>::init()),
            DataType::U8  => MaxState::U8(Max::<u8>::init()),
            DataType::U16 => MaxState::U16(Max::<u16>::init()),
            DataType::U32 => MaxState::U32(Max::<u32>::init()),
            DataType::U64 => MaxState::U64(Max::<u64>::init()),
            DataType::F32 => MaxState::F32(MaxFloat::<f32>::init()),
            DataType::F64 => MaxState::F64(MaxFloat::<f64>::init()),
            other => return Err(ExecutionError::InvalidData(
                format!("MAX is not supported for type {:?}", other)
            )),
        };
        Ok(Self { state })
    }
}

impl Aggregator for MaxAgg {
    fn update(&mut self, chunk: &ColumnChunk) -> Result<(), ExecutionError> {
        match (&mut self.state, chunk) {
            (MaxState::I8(s),  ColumnChunk::I8(v))  => Max::<i8>::update(s, v),
            (MaxState::I16(s), ColumnChunk::I16(v)) => Max::<i16>::update(s, v),
            (MaxState::I32(s), ColumnChunk::I32(v)) => Max::<i32>::update(s, v),
            (MaxState::I64(s), ColumnChunk::I64(v)) => Max::<i64>::update(s, v),
            (MaxState::U8(s),  ColumnChunk::U8(v))  => Max::<u8>::update(s, v),
            (MaxState::U16(s), ColumnChunk::U16(v)) => Max::<u16>::update(s, v),
            (MaxState::U32(s), ColumnChunk::U32(v)) => Max::<u32>::update(s, v),
            (MaxState::U64(s), ColumnChunk::U64(v)) => Max::<u64>::update(s, v),
            (MaxState::F32(s), ColumnChunk::F32(v)) => MaxFloat::<f32>::update(s, v),
            (MaxState::F64(s), ColumnChunk::F64(v)) => MaxFloat::<f64>::update(s, v),
            _ => return Err(ExecutionError::InvalidData(
                "MAX: state/chunk type mismatch (planner bug)".into()
            )),
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        match self.state {
            MaxState::I8(s)  => ColumnChunk::I8(vec![Max::<i8>::finalize(s).unwrap_or_default()]),
            MaxState::I16(s) => ColumnChunk::I16(vec![Max::<i16>::finalize(s).unwrap_or_default()]),
            MaxState::I32(s) => ColumnChunk::I32(vec![Max::<i32>::finalize(s).unwrap_or_default()]),
            MaxState::I64(s) => ColumnChunk::I64(vec![Max::<i64>::finalize(s).unwrap_or_default()]),
            MaxState::U8(s)  => ColumnChunk::U8(vec![Max::<u8>::finalize(s).unwrap_or_default()]),
            MaxState::U16(s) => ColumnChunk::U16(vec![Max::<u16>::finalize(s).unwrap_or_default()]),
            MaxState::U32(s) => ColumnChunk::U32(vec![Max::<u32>::finalize(s).unwrap_or_default()]),
            MaxState::U64(s) => ColumnChunk::U64(vec![Max::<u64>::finalize(s).unwrap_or_default()]),
            MaxState::F32(s) => ColumnChunk::F32(vec![MaxFloat::<f32>::finalize(s).unwrap_or_default()]),
            MaxState::F64(s) => ColumnChunk::F64(vec![MaxFloat::<f64>::finalize(s).unwrap_or_default()]),
        }
    }

    fn output_type(&self) -> DataType {
        match self.state {
            MaxState::I8(_)  => DataType::I8,
            MaxState::I16(_) => DataType::I16,
            MaxState::I32(_) => DataType::I32,
            MaxState::I64(_) => DataType::I64,
            MaxState::U8(_)  => DataType::U8,
            MaxState::U16(_) => DataType::U16,
            MaxState::U32(_) => DataType::U32,
            MaxState::U64(_) => DataType::U64,
            MaxState::F32(_) => DataType::F32,
            MaxState::F64(_) => DataType::F64,
        }
    }
}
