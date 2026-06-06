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

fn collect_i64(b: &[RecordBatch], col: usize) -> Vec<i64> {
    let mut out = Vec::new();
    for batch in b {
        let a = batch.column(col).as_any().downcast_ref::<Int64Array>().unwrap();
        out.extend(a.values().iter().copied());
    }
    out
}

#[test]
fn one_column() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT ts FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(collect_i64(&b, 0), vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn two_columns() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT ts, f64_c FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].num_columns(), 2);
    assert_eq!(collect_i64(&b, 0), vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(b[0].column(1).as_any().downcast_ref::<Float64Array>().unwrap().value(0), 10.0);
}

#[test]
fn all_columns() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT * FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].num_columns(), 12);
    assert_eq!(collect_i64(&b, 0), vec![1, 2, 3, 4, 5, 6]); // ts is col 0
}

#[test]
fn reversed_columns() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT f64_c, ts FROM wide", &schema, dir.path()).unwrap();
    assert_eq!(b[0].column(0).as_any().downcast_ref::<Float64Array>().unwrap().value(0), 10.0);
    assert_eq!(collect_i64(&b, 1), vec![1, 2, 3, 4, 5, 6]);
}
