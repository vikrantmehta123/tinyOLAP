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

use std::collections::HashMap;

#[test]
fn sum_by_label() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT label, SUM(f64_c) FROM wide GROUP BY label", &schema, dir.path()).unwrap();
    let batch = &b[0];
    let keys = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
    let sums = batch.column(1).as_any().downcast_ref::<Float64Array>().unwrap();
    let mut got = HashMap::new();
    for i in 0..batch.num_rows() {
        got.insert(keys.value(i).to_string(), sums.value(i));
    }
    let expected: HashMap<String, f64> =
        [("a".to_string(), 90.0), ("b".to_string(), 120.0)].into_iter().collect();
    assert_eq!(got, expected);
}

#[test]
fn count_by_label() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT label, COUNT(ts) FROM wide GROUP BY label", &schema, dir.path()).unwrap();
    let batch = &b[0];
    let keys = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
    let counts = batch.column(1).as_any().downcast_ref::<UInt64Array>().unwrap();
    let mut got = HashMap::new();
    for i in 0..batch.num_rows() {
        got.insert(keys.value(i).to_string(), counts.value(i));
    }
    let expected: HashMap<String, u64> =
        [("a".to_string(), 3), ("b".to_string(), 3)].into_iter().collect();
    assert_eq!(got, expected);
}
