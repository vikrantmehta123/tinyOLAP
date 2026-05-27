use std::sync::Arc;

use arrow::{
    array::{ArrayRef, Float64Array},
    datatypes::Field,
};

use crate::execution::{aggregation::Accumulator, executor::ExecutionError};

pub struct AvgAccumulator
{
    column_name: String,
    running_counts: Vec<u64>,
    running_sums: Vec<f64>,
}

impl AvgAccumulator
{
    pub fn new(column_name: String) -> Self {
        Self {
            column_name,
            running_counts: Vec::new(),
            running_sums: Vec::new(),
        }
    }
}

impl Accumulator for AvgAccumulator
{
    fn update(
        &mut self,
        batch: &arrow::array::RecordBatch,
        group_indices: &[u32],
        num_groups: usize,
    ) -> Result<(), ExecutionError> {
        if self.running_counts.len() < num_groups {
            self.running_counts.resize(num_groups, 0);
            self.running_sums.resize(num_groups, 0.0);
        }

        // Find the column by the runtime-supplied name.
        // Column Not Found is a planner error.
        let col_ref = match batch.column_by_name(&self.column_name) {
            Some(c) => c,
            None => {
                return Err(ExecutionError::InvalidData(format!(
                    "AvgAccumulator: column '{}' not found in batch",
                    self.column_name,
                )));
            }
        };

        // Use Arrow to cast all types to f64
        let casted =
            arrow::compute::cast(col_ref, &arrow::datatypes::DataType::Float64).map_err(|e| {
                ExecutionError::InvalidData(format!("AvgAccumulator: cast failed: {e}"))
            })?;

        let arr = casted
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("cast to Float64 must produce Float64Array");

        for (i, &gi) in group_indices.iter().enumerate() {
            let value = arr.value(i);
            self.running_sums[gi as usize] += value;
            self.running_counts[gi as usize] += 1;
        }

        Ok(())
    }

    fn merge(&mut self, batch: &arrow::array::RecordBatch, group_indices: &[u32], num_groups: usize) -> Result<(), ExecutionError> {
            todo!("merge not implemented for SumAccumulator yet")
    }

    fn output_field(&self) -> Field {
        // AVG output will always be Float64
        Field::new(
            format!("avg({})", self.column_name),
            arrow::datatypes::DataType::Float64,
            false,
        )
    }

    fn materialize(&mut self) -> ArrayRef {
        let sums = std::mem::take(&mut self.running_sums);
        let counts = std::mem::take(&mut self.running_counts);

        let averages = sums
            .into_iter()
            .zip(counts.into_iter())
            .map(|(sum, count)| sum / count as f64);

        Arc::new(Float64Array::from_iter_values(averages))
    }

    fn ensure_capacity(&mut self, num_groups: usize) {
        if self.running_counts.len() < num_groups {
            self.running_counts.resize(num_groups, 0);
            self.running_sums.resize(num_groups, 0.0);
        }
    }
}
