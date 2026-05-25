use std::fmt;
use std::ops::AddAssign;
use std::sync::Arc;

use arrow::array::{ArrayRef, ArrowNumericType, Float64Array, Int64Array, PrimitiveArray, RecordBatch, UInt64Array};
use arrow::compute::kernels::aggregate;
use arrow::datatypes::{DataType, Field, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type, Schema, UInt8Type, UInt16Type, UInt32Type, UInt64Type};

use crate::execution::executor::{ExecutionError, ExecutionPlan};

// Self::Native == the Rust primitive corresponding to this Arrow DataType
//                 (e.g. Int64Type::Native == i64, UInt32Type::Native == u32)
// Into<Self::Accumulator> == that primitive must be possible
//                            to cast into the Accumulator type
pub trait Summable: ArrowNumericType
where 
    Self::Native: Into<Self::Accumulator>
{
    // The type of running sum could be wider than the input type
    // For instance, i32 array sum can go become i64. For u32, it can 
    // become u64. Accumulator type specifies that. 
    // This accumulator type needs to have a default value (e.g. 0), 
    // it needs to support +, += operations, 
    // And it should be possible to Copy it on stack
    type Accumulator: Default + AddAssign + Copy;

    // The arrow datatype for the output schema defined using Field::new
    const OUTPUT_DATATYPE: DataType;

    // // The Accumulator would need to be cast as an array when producing output
    fn into_array(value: Self::Accumulator) -> ArrayRef;
}


// The Summable trait implementations for all the Numeric types we support
impl Summable for Int64Type {
    type Accumulator = i64;
    const OUTPUT_DATATYPE:DataType = DataType::Int64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int32Type {
    type Accumulator = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int16Type {
    type Accumulator = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;
    
    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for Int8Type {
    type Accumulator = i64;
    const OUTPUT_DATATYPE: DataType = DataType::Int64;
    
    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Int64Array::from(vec![value]))
    }
}

impl Summable for UInt64Type {
    type Accumulator = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt32Type {
    type Accumulator = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt16Type {
    type Accumulator = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for UInt8Type {
    type Accumulator = u64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(UInt64Array::from(vec![value]))
    }
}

impl Summable for Float32Type {
    type Accumulator = f64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Float64Array::from(vec![value]))
    }   
}

impl Summable for Float64Type {
    type Accumulator = f64;
    const OUTPUT_DATATYPE: DataType = DataType::UInt64;

    fn into_array(value: Self::Accumulator) -> ArrayRef {
        Arc::new(Float64Array::from(vec![value]))
    }   
}


// The Actual Query Plan operator
pub struct SumExec<T: Summable>
where 
    T::Native: Into<T::Accumulator>
{
    running_sum: T::Accumulator,
    column_name: String, 
    emitted: bool, 
    child: Box<dyn ExecutionPlan>, 
    output_schema: Arc<Schema>
}


// Implementatio 
impl<T: Summable> SumExec<T> 
where 
    T::Native: Into<T::Accumulator>
{
    pub fn new(column_name: String, child: Box<dyn ExecutionPlan>) -> Self {
        let output_field = Field::new(
            format!("sum({})", column_name), 
            T::OUTPUT_DATATYPE,
            false
        );

        let output_schema = Arc::new(Schema::new(vec![output_field]));

        Self {
            running_sum: T::Accumulator::default(),
            column_name, 
            emitted: false, 
            child, 
            output_schema
        }
    }      
}

impl<T: Summable> fmt::Display for SumExec<T> 
where 
    T::Native: Into<T::Accumulator>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl<T: Summable> ExecutionPlan for SumExec<T>
where 
    T::Native: Into<T::Accumulator>
{
    fn next_batch(&mut self) -> Option<Result<RecordBatch, ExecutionError>> {
        
        if self.emitted {
            return None;
        }

        loop {
            let batch = match self.child.next_batch() {
                None             => break,
                Some(Ok(b))      => b,
                Some(Err(e))     => return Some(Err(e)),
            };    

            // Find the column by the runtime-supplied name.
            // Column Not Found is a planner error.
            let col_ref = match batch.column_by_name(&self.column_name) {
                Some(c) => c,
                None => return Some(Err(ExecutionError::InvalidData(format!(
                    "SumExec: column '{}' not found in batch",
                    self.column_name,
                )))),
            };

            // Downcast the array to the primitive Arrow type
            let arr = col_ref
                .as_any()
                .downcast_ref::<PrimitiveArray<T>>()
                .expect("SumExec: column type does not match T — planner bug");

            // Arrow's SIMD sum kernel
            if let Some(partial) = aggregate::sum(arr) {
                // .into() widens T::Native into T::Accumulator.
                self.running_sum += partial.into();
            }

        }
        
        // Build the one-row output batch.
        let arr: ArrayRef = T::into_array(self.running_sum);
        let batch = match RecordBatch::try_new(self.output_schema.clone(), vec![arr]) {
            Ok(b)  => b,
            Err(e) => return Some(Err(e.into())),
        };

        self.emitted = true;
        Some(Ok(batch))
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{}Sum({})", indent, self.column_name)?;
        self.child.fmt_indented(f, depth + 1)
    }
}