use std::ops::AddAssign;
use std::sync::Arc;

use arrow::array::{ArrayRef, ArrowNumericType, Float64Array, Int64Array, PrimitiveArray, RecordBatch, UInt64Array};
use arrow::compute::kernels::aggregate;
use arrow::datatypes::{DataType, Field, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type};

use crate::execution::aggregation::Accumulator;
use crate::execution::executor::{ExecutionError};

// Self::Native == the Rust primitive corresponding to this Arrow DataType
//                 (e.g. Int64Type::Native == i64, UInt32Type::Native == u32)
// Into<Self::Widened> == that primitive must be possible
//                            to cast into the Widened type
pub trait Summable: ArrowNumericType
where 
    Self::Native: Into<Self::Widened>
{
    // The type of running sum could be wider than the input type
    // For instance, i32 array sum can go become i64. For u32, it can 
    // become u64. widened type specifies that. 
    // This widened type needs to have a default value (e.g. 0), 
    // it needs to support +, += operations, 
    // And it should be possible to Copy it on stack
    type Widened: Default + AddAssign + Copy;

    // The arrow datatype for the output schema defined using Field::new
    const OUTPUT_DATATYPE: DataType;

    // // The Widened would need to be cast as an array when producing output
    fn into_array(value: Self::Widened) -> ArrayRef;
}


// The Summable trait implementations for all the Numeric types we support
impl Summable for Int64Type {
    type Widened = i64;
    const OUTPUT_DATATYPE:DataType = DataType::Int64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int32Type {
    type Widened = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int16Type {
    type Widened = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;
    
    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int8Type {
    type Widened = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;
    
    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for UInt64Type {
    type Widened = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt32Type {
    type Widened = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt16Type {
    type Widened = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt8Type {
    type Widened = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for Float32Type {
    type Widened = f64;
    const OUTPUT_DATATYPE: DataType = DataType::Float64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Float64Array::from(vec![value]))
    }   
}

impl Summable for Float64Type {
    type Widened = f64;
    const OUTPUT_DATATYPE: DataType = DataType::Float64;

    fn into_array(value: Self::Widened) -> ArrayRef {
        Arc::new(Float64Array::from(vec![value]))
    }   
}


// Implementation
pub struct SumAccumulator<T: Summable>
where 
    T::Native: Into<T::Widened>,
{
    running_sum: T::Widened,
    column_name: String,
}

impl<T: Summable> SumAccumulator<T>
where 
    T::Native: Into<T::Widened>
{
    pub fn new(column_name: String) -> Self {
        Self {
            column_name, 
            running_sum: T::Widened::default(),
        }
    }
}

impl<T:Summable> Accumulator for SumAccumulator<T>
where 
    T::Native: Into<T::Widened>
{
    fn update(&mut self, batch: &RecordBatch) -> Result<(), ExecutionError> {
        // Find the column by the runtime-supplied name.
        // Column Not Found is a planner error.
        let col_ref = match batch.column_by_name(&self.column_name) {
            Some(c) => c,
            None => return Err(ExecutionError::InvalidData(format!(
                "SumAccumulator: column '{}' not found in batch",
                self.column_name,
            ))),
        };

        // Downcast the array to the primitive Arrow type
        let arr = col_ref
            .as_any()
            .downcast_ref::<PrimitiveArray<T>>()
            .expect("SumExec: column type does not match T — planner bug");

        // Arrow's SIMD sum kernel
        if let Some(partial) = aggregate::sum(arr) {
            // .into() widens T::Native into T::Widened.
            self.running_sum += partial.into();
        }

        Ok(())
    }

    fn output_field(&self) -> Field {
        Field::new(
            format!("sum({})", self.column_name), 
            T::OUTPUT_DATATYPE,
            false
        )
    }

    fn finalize(&mut self) -> ArrayRef {
        T::into_array(self.running_sum)
    }
}