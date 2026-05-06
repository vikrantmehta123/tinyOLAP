//! ScalarValue Type
//! 
//! This type is used in GROUP BY statements as Key
//! GROUP BY is nothing but a key-value pairs.
//! The key then becomes: Vec<ScalarValue>
//! Each needs to be hashable.
//! 
//! TODO: Need to check if we can define a single DataType 
//! and reuse it everywhere instead of defining data types in 
//! multiple locations.

use std::hash::{Hash, Hasher};
use crate::storage::column_chunk::ColumnChunk;

/// A single row's value from a column. Needs Hash + Eq to be used as a HashMap key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScalarValue {
    I8(i8), I16(i16), I32(i32), I64(i64),
    U8(u8), U16(u16), U32(u32), U64(u64),
    F32(HashableF32), F64(HashableF64),
    Bool(bool),
    Str(String),
}

/// Tuple of values from the GROUP BY columns for one row.
pub type GroupKey = Vec<ScalarValue>;

// f32 doesn't implement Hash or Eq because of NaN. We use bit representation:
// NaN == NaN for grouping purposes, which matches standard database behaviour.
#[derive(Debug, Clone, Copy)]
pub struct HashableF32(pub f32);

impl PartialEq for HashableF32 {
    fn eq(&self, other: &Self) -> bool { self.0.to_bits() == other.0.to_bits() }
}
impl Eq for HashableF32 {}
impl Hash for HashableF32 {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.to_bits().hash(state); }
}

#[derive(Debug, Clone, Copy)]
pub struct HashableF64(pub f64);

impl PartialEq for HashableF64 {
    fn eq(&self, other: &Self) -> bool { self.0.to_bits() == other.0.to_bits() }
}
impl Eq for HashableF64 {}
impl Hash for HashableF64 {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.to_bits().hash(state); }
}

impl ScalarValue {
    /// Extract the value at `row` from a column. Used during the drain phase.
    pub fn from_chunk(chunk: &ColumnChunk, row: usize) -> Self {
        match chunk {
            ColumnChunk::I8(v)  => ScalarValue::I8(v[row]),
            ColumnChunk::I16(v) => ScalarValue::I16(v[row]),
            ColumnChunk::I32(v) => ScalarValue::I32(v[row]),
            ColumnChunk::I64(v) => ScalarValue::I64(v[row]),
            ColumnChunk::U8(v)  => ScalarValue::U8(v[row]),
            ColumnChunk::U16(v) => ScalarValue::U16(v[row]),
            ColumnChunk::U32(v) => ScalarValue::U32(v[row]),
            ColumnChunk::U64(v) => ScalarValue::U64(v[row]),
            ColumnChunk::F32(v) => ScalarValue::F32(HashableF32(v[row])),
            ColumnChunk::F64(v) => ScalarValue::F64(HashableF64(v[row])),
            ColumnChunk::Bool(v) => ScalarValue::Bool(v[row]),
            ColumnChunk::Str(v)  => ScalarValue::Str(v[row].clone()),
        }
    }

    /// Build a column from a vec of same-typed scalars. Used when assembling the output Batch.
    /// Panics on type mismatch — callers are responsible for passing uniform vecs.
    pub fn build_column(values: Vec<ScalarValue>) -> ColumnChunk {
        // All values come from the same schema column so the first element
        // determines the variant; the rest are guaranteed to match.
        match values.first() {
            None => ColumnChunk::I8(vec![]),
            Some(ScalarValue::I8(_))   => ColumnChunk::I8(values.into_iter().map(|v| match v { ScalarValue::I8(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::I16(_))  => ColumnChunk::I16(values.into_iter().map(|v| match v { ScalarValue::I16(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::I32(_))  => ColumnChunk::I32(values.into_iter().map(|v| match v { ScalarValue::I32(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::I64(_))  => ColumnChunk::I64(values.into_iter().map(|v| match v { ScalarValue::I64(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::U8(_))   => ColumnChunk::U8(values.into_iter().map(|v| match v { ScalarValue::U8(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::U16(_))  => ColumnChunk::U16(values.into_iter().map(|v| match v { ScalarValue::U16(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::U32(_))  => ColumnChunk::U32(values.into_iter().map(|v| match v { ScalarValue::U32(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::U64(_))  => ColumnChunk::U64(values.into_iter().map(|v| match v { ScalarValue::U64(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::F32(_))  => ColumnChunk::F32(values.into_iter().map(|v| match v { ScalarValue::F32(x) => x.0, _ => unreachable!() }).collect()),
            Some(ScalarValue::F64(_))  => ColumnChunk::F64(values.into_iter().map(|v| match v { ScalarValue::F64(x) => x.0, _ => unreachable!() }).collect()),
            Some(ScalarValue::Bool(_)) => ColumnChunk::Bool(values.into_iter().map(|v| match v { ScalarValue::Bool(x) => x, _ => unreachable!() }).collect()),
            Some(ScalarValue::Str(_))  => ColumnChunk::Str(values.into_iter().map(|v| match v { ScalarValue::Str(x) => x, _ => unreachable!() }).collect()),
        }
    }
}
