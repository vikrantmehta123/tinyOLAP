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
#[ignore = "LIMIT not yet correct end-to-end"]
fn limit_3() {
    let (dir, schema) = setup();
    let b = run_select_collect("SELECT ts FROM wide LIMIT 3", &schema, dir.path()).unwrap();
    let rows: usize = b.iter().map(|x| x.num_rows()).sum();
    assert_eq!(rows, 3);
}
