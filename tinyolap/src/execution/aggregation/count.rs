use std::sync::Arc;

use arrow::{
    array::{ArrayRef, RecordBatch, UInt64Array},
    datatypes::{DataType, Field},
};

use crate::execution::{aggregation::Accumulator, executor::ExecutionError};

pub struct CountAccumulator {
    counts: Vec<u64>, // Count of values per GROUP BY group
    column_name: String,
}

impl CountAccumulator {
    pub fn new(column_name: String) -> Self {
        Self {
            column_name,
            counts: Vec::new(),
        }
    }
}

impl Accumulator for CountAccumulator {
    fn update(
        &mut self,
        batch: &RecordBatch,
        group_indices: &[u32],
        num_groups: usize,
    ) -> Result<(), ExecutionError> {
        let _batch = batch; // COUNT doesn't read column values — even with groups
        if self.counts.len() < num_groups {
            self.counts.resize(num_groups, 0);
        }

        // No GROUP BY clause
        if num_groups == 1 {
            self.counts[0] += group_indices.len() as u64;
            return Ok(());
        }

        // GROUP BY clause is present
        for &gi in group_indices {
            self.counts[gi as usize] += 1; // no branch
        }
        Ok(())
    }

    fn merge(&mut self, batch: &RecordBatch, group_indices: &[u32], num_groups: usize) -> Result<(), ExecutionError> {
        if self.counts.len() < num_groups {
            self.counts.resize(num_groups, 0);
        }

        let field = self.output_field();
        let colname = field.name();
        let col_ref = batch.column_by_name(colname).ok_or_else(|| ExecutionError::InvalidData(colname.to_string()))?; 

        let arr = col_ref
            .as_any()
            .downcast_ref::<UInt64Array>()
            .expect("MergeCountExec: The downcast array type doesn't match.");

        // Merge operation for COUNT function is the sum of values
        for (row_idx, &gi) in group_indices.iter().enumerate() {
            self.counts[gi as usize] += arr.value(row_idx);
        }
        Ok(())
    }

    fn materialize(&mut self) -> ArrayRef {
        Arc::new(UInt64Array::from(std::mem::take(&mut self.counts)))
    }

    fn output_field(&self) -> Field {
        Field::new(
            format!("count({})", self.column_name),
            DataType::UInt64,
            false,
        )
    }

    fn ensure_capacity(&mut self, num_groups: usize) {
        if self.counts.len() < num_groups {
            self.counts.resize(num_groups, 0);
        }
    }
}
