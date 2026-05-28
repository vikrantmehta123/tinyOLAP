use std::sync::Arc;

use arrow::{
    array::{ArrayRef, Float64Array, StructArray, UInt64Array},
    datatypes::{DataType, Field},
};

use crate::execution::{aggregation::Accumulator, executor::ExecutionError};

pub struct AvgAccumulator
{
    column_name: String,
    running_counts: Vec<u64>,
    running_sums: Vec<f64>,
    is_partial: bool,
}

impl AvgAccumulator
{
    pub fn new(column_name: String, is_partial: bool) -> Self {
        Self {
            column_name,
            running_counts: Vec::new(),
            running_sums: Vec::new(),
            is_partial,
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
        self.ensure_capacity(num_groups);
        // The name matches whatever output_field() generated in the partial phase
        let col_name = format!("avg({})", self.column_name);
        let col_ref = batch.column_by_name(&col_name).unwrap();
        // In the merge phase, the incoming column is a StructArray containing [sum, count]
        let struct_arr = col_ref.as_any().downcast_ref::<StructArray>().unwrap();
        
        // Extract the internal arrays
        let sums_arr = struct_arr.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
        let counts_arr = struct_arr.column(1).as_any().downcast_ref::<UInt64Array>().unwrap();
        // Merge the partial states!
        for (i, &gi) in group_indices.iter().enumerate() {
            self.running_sums[gi as usize] += sums_arr.value(i);
            self.running_counts[gi as usize] += counts_arr.value(i);
        }
        Ok(())
    }

    fn output_field(&self) -> Field {
        let name = format!("avg({})", self.column_name);

        // When returning partial results, we need to return both count and sum
        // Wrap both in a Field of StructArray
        if self.is_partial {
            let fields = vec![
                Field::new("sum", DataType::Float64, false),
                Field::new("count", DataType::UInt64, false),
            ];
            Field::new(name, arrow::datatypes::DataType::Struct(fields.into()), false)
        } else {
            Field::new(name, arrow::datatypes::DataType::Float64, false)
        }
    }

    fn materialize(&mut self) -> ArrayRef {
        let sums = std::mem::take(&mut self.running_sums);
        let counts = std::mem::take(&mut self.running_counts);

        if self.is_partial {
            let sums_arr = Arc::new(Float64Array::from(sums)) as ArrayRef;
            let counts_arr = Arc::new(UInt64Array::from(counts)) as ArrayRef;
        
            let fields = vec![
                Field::new("sum", DataType::Float64, false),
                Field::new("count", DataType::UInt64, false),
            ];

            Arc::new(StructArray::from(vec![
                (Arc::new(fields[0].clone()), sums_arr),
                (Arc::new(fields[1].clone()), counts_arr),
            ]))
        } else {
            let averages = sums
                .into_iter()
                .zip(counts.into_iter())
                .map(|(sum, count)| sum / count as f64);

            Arc::new(Float64Array::from_iter_values(averages))
        }
    }

    fn ensure_capacity(&mut self, num_groups: usize) {
        if self.running_counts.len() < num_groups {
            self.running_counts.resize(num_groups, 0);
            self.running_sums.resize(num_groups, 0.0);
        }
    }
}
