use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float32Array, Float64Array,
    Int8Array, Int16Array, Int32Array, Int64Array,
    RecordBatch, StringArray,
    UInt8Array, UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::{Field, Schema};
use sqlparser::ast::{Expr, SetExpr, Statement, Value};

use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};

pub fn build_record_batch(
    stmt: &Statement,
    schema: &TableSchema,
) -> Result<RecordBatch, String> {
    let rows = extract_rows(stmt)?;

    // Validate row widths up front — fail fast on shape errors.
    for (i, row) in rows.iter().enumerate() {
        if row.len() != schema.columns.len() {
            return Err(format!(
                "row {}: expected {} values, got {}",
                i, schema.columns.len(), row.len()
            ));
        }
    }

    // Transpose row-major Vec<Vec<Expr>> into one typed Arrow array per column.
    let arrays: Vec<ArrayRef> = schema.columns
        .iter()
        .enumerate()
        .map(|(col_idx, col)| build_column_array(col, col_idx, rows))
        .collect::<Result<_, _>>()?;

    // Arrow schema from our schema — uses the lifted DataType::to_arrow().
    let fields: Vec<Field> = schema.columns.iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));

    RecordBatch::try_new(arrow_schema, arrays).map_err(|e| e.to_string())
}

fn extract_rows(stmt: &Statement) -> Result<&Vec<Vec<Expr>>, String> {
    let insert = match stmt {
        Statement::Insert(i) => i,
        _ => return Err("not an INSERT statement".into()),
    };
    let source = insert.source.as_ref()
        .ok_or("INSERT without VALUES is not supported")?;
    match source.body.as_ref() {
        SetExpr::Values(v) => Ok(&v.rows),
        _ => Err("INSERT only supports VALUES, not INSERT-from-SELECT".into()),
    }
}

fn build_column_array(
    col: &ColumnSchema,
    col_idx: usize,
    rows: &[Vec<Expr>],
) -> Result<ArrayRef, String> {
    match col.data_type {
        DataType::I8   => Ok(Arc::new(Int8Array  ::from(parse_numeric::<i8> (rows, col_idx, &col.name)?))),
        DataType::I16  => Ok(Arc::new(Int16Array ::from(parse_numeric::<i16>(rows, col_idx, &col.name)?))),
        DataType::I32  => Ok(Arc::new(Int32Array ::from(parse_numeric::<i32>(rows, col_idx, &col.name)?))),
        DataType::I64  => Ok(Arc::new(Int64Array ::from(parse_numeric::<i64>(rows, col_idx, &col.name)?))),
        DataType::U8   => Ok(Arc::new(UInt8Array ::from(parse_numeric::<u8> (rows, col_idx, &col.name)?))),
        DataType::U16  => Ok(Arc::new(UInt16Array::from(parse_numeric::<u16>(rows, col_idx, &col.name)?))),
        DataType::U32  => Ok(Arc::new(UInt32Array::from(parse_numeric::<u32>(rows, col_idx, &col.name)?))),
        DataType::U64  => Ok(Arc::new(UInt64Array::from(parse_numeric::<u64>(rows, col_idx, &col.name)?))),
        DataType::F32  => Ok(Arc::new(Float32Array::from(parse_numeric::<f32>(rows, col_idx, &col.name)?))),
        DataType::F64  => Ok(Arc::new(Float64Array::from(parse_numeric::<f64>(rows, col_idx, &col.name)?))),
        DataType::Bool => Ok(Arc::new(BooleanArray::from(parse_bool(rows, col_idx, &col.name)?))),
        DataType::Str  => Ok(Arc::new(StringArray::from(parse_str(rows, col_idx, &col.name)?))),
    }
}

// One generic numeric parser. T: FromStr (all 10 numeric types satisfy this).
// Inner T::Err: Display so we can format parse errors uniformly.
fn parse_numeric<T: FromStr>(
    rows: &[Vec<Expr>],
    col_idx: usize,
    col_name: &str,
) -> Result<Vec<T>, String>
where
    T::Err: std::fmt::Display,
{
    rows.iter().enumerate().map(|(row_idx, row)| {
        match &row[col_idx] {
            Expr::Value(vws) => match &vws.value {
                Value::Number(s, _) => s.parse::<T>().map_err(|e| {
                    format!("col {}, row {}: cannot parse '{}' as numeric: {}",
                        col_name, row_idx, s, e)
                }),
                other => Err(format!(
                    "col {}, row {}: expected number, got {:?}",
                    col_name, row_idx, other,
                )),
            },
            other => Err(format!(
                "col {}, row {}: expected literal, got {:?}",
                col_name, row_idx, other,
            )),
        }
    }).collect()
}

fn parse_bool(
    rows: &[Vec<Expr>],
    col_idx: usize,
    col_name: &str,
) -> Result<Vec<bool>, String> {
    rows.iter().enumerate().map(|(row_idx, row)| {
        match &row[col_idx] {
            Expr::Value(vws) => match &vws.value {
                Value::Boolean(b) => Ok(*b),
                other => Err(format!(
                    "col {}, row {}: expected boolean, got {:?}",
                    col_name, row_idx, other,
                )),
            },
            other => Err(format!(
                "col {}, row {}: expected literal, got {:?}",
                col_name, row_idx, other,
            )),
        }
    }).collect()
}

fn parse_str(
    rows: &[Vec<Expr>],
    col_idx: usize,
    col_name: &str,
) -> Result<Vec<String>, String> {
    rows.iter().enumerate().map(|(row_idx, row)| {
        match &row[col_idx] {
            Expr::Value(vws) => match &vws.value {
                Value::SingleQuotedString(s) => Ok(s.clone()),
                other => Err(format!(
                    "col {}, row {}: expected single-quoted string, got {:?}",
                    col_name, row_idx, other,
                )),
            },
            other => Err(format!(
                "col {}, row {}: expected literal, got {:?}",
                col_name, row_idx, other,
            )),
        }
    }).collect()
}
