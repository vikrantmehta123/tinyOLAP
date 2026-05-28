use std::sync::Arc;

use arrow::{
    array::{ArrayRef, ArrowNumericType, PrimitiveArray},
    compute::kernels::aggregate,
    datatypes::Field,
};

use crate::execution::{aggregation::Accumulator, executor::ExecutionError};


/// NaN handling is order-dependent and may differ between the SIMD fast path
/// (`aggregate::max`) and the scalar loop. Acceptable for tinyOLAP today; revisit
/// if float columns become common in aggregations.
pub struct MaxAccumulator<T: ArrowNumericType>
where
    T::Native: PartialOrd,
{
    has_value: Vec<bool>,
    running_maximums: Vec<T::Native>,
    column_name: String,
}

impl<T: ArrowNumericType> MaxAccumulator<T>
where
    T::Native: PartialOrd,
{
    pub fn new(column_name: String) -> Self {
        Self {
            column_name,
            running_maximums: Vec::new(),
            has_value: Vec::new(),
        }
    }
}

impl<T: ArrowNumericType> Accumulator for MaxAccumulator<T>
where
    T::Native: PartialOrd,
{
    fn update(
        &mut self,
        batch: &arrow::array::RecordBatch,
        group_indices: &[u32],
        num_groups: usize,
    ) -> Result<(), ExecutionError> {
        if self.has_value.len() < num_groups {
            self.has_value.resize(num_groups, false);
            self.running_maximums
                .resize(num_groups, T::Native::default());
        }

        // Find the column by the runtime-supplied name.
        // Column Not Found is a planner error.
        let col_ref = match batch.column_by_name(&self.column_name) {
            Some(c) => c,
            None => {
                return Err(ExecutionError::InvalidData(format!(
                    "MaxAccumulator: column '{}' not found in batch",
                    self.column_name,
                )));
            }
        };

        // Downcast the array to the primitive Arrow type
        let arr = col_ref
            .as_any()
            .downcast_ref::<PrimitiveArray<T>>()
            .expect("MaxExec: column type does not match T — planner bug");

        // No GROUP BY => we can use SIMD
        if num_groups == 1 {
            if let Some(partial) = aggregate::max(arr) {
                if self.has_value[0] {
                    if partial > self.running_maximums[0] {
                        self.running_maximums[0] = partial;
                    }
                } else {
                    self.running_maximums[0] = partial;
                    self.has_value[0] = true;
                }
            }
            return Ok(());
        }

        for (i, &gi) in group_indices.iter().enumerate() {
            let value = arr.value(i);

            if !self.has_value[gi as usize] {
                self.running_maximums[gi as usize] = value;
                self.has_value[gi as usize] = true;
                continue;
            }

            if value > self.running_maximums[gi as usize] {
                self.running_maximums[gi as usize] = value;
            }
        }

        Ok(())
    }

    fn merge(&mut self, batch: &arrow::array::RecordBatch, group_indices: &[u32], num_groups: usize) -> Result<(), ExecutionError> {
        if self.has_value.len() < num_groups {
            self.has_value.resize(num_groups, false);
            self.running_maximums
                .resize(num_groups, T::Native::default());
        }

        let field = self.output_field();
        let colname = field.name();
        let col_ref = batch.column_by_name(colname).ok_or_else(|| ExecutionError::InvalidData(colname.to_string()))?; 

        let arr = col_ref
            .as_any()
            .downcast_ref::<PrimitiveArray<T>>()
            .expect("MergeMaxExec: The downcast array type doesn't match.");
        
        // No GROUP BY => we can use SIMD
        if num_groups == 1 {
            if let Some(partial) = aggregate::max(arr) {
                if self.has_value[0] {
                    if partial > self.running_maximums[0] {
                        self.running_maximums[0] = partial;
                    }
                } else {
                    self.running_maximums[0] = partial;
                    self.has_value[0] = true;
                }
            }
            return Ok(());
        }

        for (i, &gi) in group_indices.iter().enumerate() {
            let value = arr.value(i);

            if !self.has_value[gi as usize] {
                self.running_maximums[gi as usize] = value;
                self.has_value[gi as usize] = true;
                continue;
            }

            if value > self.running_maximums[gi as usize] {
                self.running_maximums[gi as usize] = value;
            }
        }

        Ok(())
    }

    fn output_field(&self) -> Field {
        Field::new(format!("max({})", self.column_name), T::DATA_TYPE, false)
    }

    fn materialize(&mut self) -> ArrayRef {
        Arc::new(PrimitiveArray::<T>::from_iter_values(std::mem::take(
            &mut self.running_maximums,
        )))
    }

    fn ensure_capacity(&mut self, num_groups: usize) {
        if self.has_value.len() < num_groups {
            self.has_value.resize(num_groups, false);
            self.running_maximums
                .resize(num_groups, T::Native::default());
        }
    }
}
