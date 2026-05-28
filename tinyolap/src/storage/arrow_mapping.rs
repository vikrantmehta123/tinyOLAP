use arrow::array::{
    ArrayRef, BooleanArray, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array,
    Int64Array, UInt8Array, UInt16Array, UInt32Array, UInt64Array
};
use std::{i32, sync::Arc};


pub trait ArrowMappable {
    fn into_array(values: Vec<Self>) -> ArrayRef
    where Self: Sized;
}

impl ArrowMappable for i64 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Int64Array::from(values))
    }
}

impl ArrowMappable for i32 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Int32Array::from(values))
    }
}

impl ArrowMappable for i16 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Int16Array::from(values))
    }
}

impl ArrowMappable for i8 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Int8Array::from(values))
    }
}

impl ArrowMappable for u8 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(UInt8Array::from(values))
    }
}

impl ArrowMappable for u16 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(UInt16Array::from(values))
    }
}


impl ArrowMappable for u32 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(UInt32Array::from(values))
    }
}


impl ArrowMappable for u64 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(UInt64Array::from(values))
    }
}


impl ArrowMappable for f32 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Float32Array::from(values))
    }
}

impl ArrowMappable for f64 {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(Float64Array::from(values))
    }
}

impl ArrowMappable for bool {
    fn into_array(values: Vec<Self>) -> ArrayRef {
        Arc::new(BooleanArray::from(values))
    }
}