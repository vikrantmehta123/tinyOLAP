use arrow::array::{
    ArrayRef, Float32Array, Float64Array, Int8Array, Int16Array, Int32Array, Int64Array,
    RecordBatch, UInt8Array, UInt16Array, UInt32Array, UInt64Array
};

use crate::{
    execution::executor::ExecutionError,
    physical_plan::physical_operators::{CmpOp, LiteralValue, LogicalOp, PhysicalExpr},
};
use arrow::array::{BooleanArray, Datum, Scalar, StringArray};

pub enum ColumnarValue {
    Array(ArrayRef),
    Scalar(LiteralValue),
}

impl ColumnarValue {
    fn to_arrow_datum(&self) -> Box<dyn Datum + '_> {
        match self {
            ColumnarValue::Array(arr) => Box::new(arr.clone()),
            ColumnarValue::Scalar(lit) => match lit {
                LiteralValue::I8(v)   => Box::new(Scalar::new(Int8Array::from(vec![*v]))),
                LiteralValue::I16(v)  => Box::new(Scalar::new(Int16Array::from(vec![*v]))),
                LiteralValue::I32(v)  => Box::new(Scalar::new(Int32Array::from(vec![*v]))),
                LiteralValue::I64(v)  => Box::new(Scalar::new(Int64Array::from(vec![*v]))),
                LiteralValue::U8(v)   => Box::new(Scalar::new(UInt8Array::from(vec![*v]))),
                LiteralValue::U16(v)  => Box::new(Scalar::new(UInt16Array::from(vec![*v]))),
                LiteralValue::U32(v)  => Box::new(Scalar::new(UInt32Array::from(vec![*v]))),
                LiteralValue::U64(v)  => Box::new(Scalar::new(UInt64Array::from(vec![*v]))),
                LiteralValue::F32(v)  => Box::new(Scalar::new(Float32Array::from(vec![*v]))),
                LiteralValue::F64(v)  => Box::new(Scalar::new(Float64Array::from(vec![*v]))),
                LiteralValue::Bool(v) => Box::new(Scalar::new(BooleanArray::from(vec![*v]))),
                LiteralValue::Str(v)  => Box::new(Scalar::new(StringArray::from(vec![v.clone()]))),
                LiteralValue::Null    => panic!("NULL literal not supported"),
            },
        }
    }
}

pub fn evaluate_predicate(
    expr: &PhysicalExpr,
    batch: &RecordBatch,
) -> Result<BooleanArray, ExecutionError> {
    match expr {
        PhysicalExpr::Compare { left, op, right } => {
            let l_cv = evaluate_operand(left, batch)?;
            let r_cv = evaluate_operand(right, batch)?;
            let l = l_cv.to_arrow_datum();
            let r = r_cv.to_arrow_datum();
            use arrow::compute::kernels::cmp;
            let result = match op {
                CmpOp::Eq => cmp::eq(&*l, &*r),
                CmpOp::NotEq => cmp::neq(&*l, &*r),
                CmpOp::Lt => cmp::lt(&*l, &*r),
                CmpOp::LtEq => cmp::lt_eq(&*l, &*r),
                CmpOp::Gt => cmp::gt(&*l, &*r),
                CmpOp::GtEq => cmp::gt_eq(&*l, &*r),
            };
            Ok(result?)
        }

        PhysicalExpr::Logical { left, op, right } => {
            let l = evaluate_predicate(left, batch)?;
            let r = evaluate_predicate(right, batch)?;
            use arrow::compute::kernels::boolean;
            let result = match op {
                LogicalOp::And => boolean::and(&l, &r),
                LogicalOp::Or => boolean::or(&l, &r),
            };
            Ok(result?)
        }
        PhysicalExpr::Column(_) | PhysicalExpr::Literal(_) => Err(ExecutionError::InvalidData(
            "predicate must be a comparison or logical expression".into(),
        )),
    }
}

fn evaluate_operand(
    expr: &PhysicalExpr,
    batch: &RecordBatch,
) -> Result<ColumnarValue, ExecutionError> {
    match expr {
        PhysicalExpr::Column(name) => {
            let idx = batch.schema().index_of(name)?;
            let res = batch.column(idx).clone();
            Ok(ColumnarValue::Array(res))
        }
        PhysicalExpr::Literal(lit) => Ok(ColumnarValue::Scalar(lit.clone())),
        _ => Err(ExecutionError::InvalidData(
            "compare operand must be a column or literal".to_string(),
        )),
    }
}
