use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array,
    Int8Array, RecordBatch, StringArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow::datatypes::{Field, Schema};
use tempfile::TempDir;

use tinyolap::catalog::schema::TableSchema;
use tinyolap::run_select_collect;
use tinyolap::storage::table_writer::TableWriter;

use crate::common::wide_schema;

/// Small single-part table: 6 rows. ts/i*/u* = 1..6, f* = 10..60, label a/b.
fn setup() -> (TempDir, TableSchema) {
    let schema = wide_schema();
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(vec![1i64, 2, 3, 4, 5, 6])),
        Arc::new(Int8Array::from(vec![1i8, 2, 3, 4, 5, 6])),
        Arc::new(Int16Array::from(vec![1i16, 2, 3, 4, 5, 6])),
        Arc::new(Int32Array::from(vec![1i32, 2, 3, 4, 5, 6])),
        Arc::new(UInt8Array::from(vec![1u8, 2, 3, 4, 5, 6])),
        Arc::new(UInt16Array::from(vec![1u16, 2, 3, 4, 5, 6])),
        Arc::new(UInt32Array::from(vec![1u32, 2, 3, 4, 5, 6])),
        Arc::new(UInt64Array::from(vec![1u64, 2, 3, 4, 5, 6])),
        Arc::new(Float32Array::from(vec![10.0f32, 20.0, 30.0, 40.0, 50.0, 60.0])),
        Arc::new(Float64Array::from(vec![10.0f64, 20.0, 30.0, 40.0, 50.0, 60.0])),
        Arc::new(BooleanArray::from(vec![true, false, true, false, true, false])),
        Arc::new(StringArray::from(vec!["a", "b", "a", "b", "a", "b"])),
    ];
    let fields: Vec<Field> = schema
        .columns
        .iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(arrow_schema, arrays).unwrap();

    let dir = TempDir::new().unwrap();
    TableSchema::create(dir.path(), &schema).unwrap();
    let w = TableWriter::open(dir.path().to_path_buf()).unwrap();
    w.insert(batch).unwrap();
    (dir, schema)
}

#[test]
fn sum_i8() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(i8_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_i16() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(i16_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_i32() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(i32_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_i64() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(ts) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Int64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_u8() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(u8_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_u16() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(u16_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_u32() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(u32_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_u64() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(u64_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt64Array>().unwrap().value(0), 21);
}

#[test]
fn sum_f32() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(f32_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Float64Array>().unwrap().value(0), 210.0);
}

#[test]
fn sum_f64() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT SUM(f64_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Float64Array>().unwrap().value(0), 210.0);
}

#[test]
fn count() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT COUNT(ts) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt64Array>().unwrap().value(0), 6);
}

#[test]
fn min_i16() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT MIN(i16_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Int16Array>().unwrap().value(0), 1);
}

#[test]
fn max_u32() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT MAX(u32_c) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<UInt32Array>().unwrap().value(0), 6);
}

#[test]
fn avg() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT AVG(ts) FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Float64Array>().unwrap().value(0), 3.5);
}
