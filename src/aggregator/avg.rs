//! Avg aggregate over numeric types.
//!
//! Always accumulates in f64 and returns f64 regardless of input type —
//! casting integers up avoids both overflow and a per-type state enum.
//! No generic Avg<T> math struct: the accumulation is trivial enough that
//! splitting it out would add indirection for no gain.

use std::vec;

use crate::aggregator::Aggregator;
use crate::processors::processor::ExecutionError;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::DataType;

pub struct AvgAgg {
    sum: Vec<f64>,
    count: Vec<u64>,
}

impl AvgAgg {
    pub fn new(input: DataType) -> Result<Self, ExecutionError> {
        match input {
            DataType::Bool | DataType::Str => Err(ExecutionError::InvalidData(format!(
                "AVG is not supported for type {:?}",
                input
            ))),
            _ => Ok(Self {
                sum: vec![],
                count: vec![],
            }),
        }
    }
}

impl Aggregator for AvgAgg {
    fn update(
        &mut self,
        chunk: &ColumnChunk,
        group_ids: &[u32],
        n_groups: usize,
    ) -> Result<(), ExecutionError> {
        if self.sum.len() < n_groups {
            self.sum.resize(n_groups, 0.0);
            self.count.resize(n_groups, 0);
        }
        match chunk {
            ColumnChunk::I8(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::I16(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::I32(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::I64(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::U8(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::U16(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::U32(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::U64(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::F32(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val as f64;
                    self.count[gid as usize] += 1;
                }
            }
            ColumnChunk::F64(v) => {
                for (&val, &gid) in v.iter().zip(group_ids) {
                    self.sum[gid as usize] += val;
                    self.count[gid as usize] += 1;
                }
            }
            _ => {
                return Err(ExecutionError::InvalidData(
                    "AVG: unsupported column type (planner bug)".into(),
                ));
            }
        }
        Ok(())
    }

    fn finalize(&mut self) -> ColumnChunk {
        let result = self
            .sum
            .iter()
            .zip(self.count.iter())
            .map(|(&s, &c)| if c == 0 { 0.0 } else { s / c as f64 })
            .collect();
        ColumnChunk::F64(result)
    }

    fn output_type(&self) -> DataType {
        DataType::F64
    }
}
