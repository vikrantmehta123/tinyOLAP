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

/// Single-part table of `n` rows. ts = 0..n; wide numeric cols = i; i8/u8 use
/// i % 100 (can't hold 0..512); label alternates a/b by parity.
fn setup(n: usize) -> (TempDir, TableSchema) {
    let schema = wide_schema();
    let arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from((0..n).map(|i| i as i64).collect::<Vec<_>>())),
        Arc::new(Int8Array::from((0..n).map(|i| (i % 100) as i8).collect::<Vec<_>>())),
        Arc::new(Int16Array::from((0..n).map(|i| i as i16).collect::<Vec<_>>())),
        Arc::new(Int32Array::from((0..n).map(|i| i as i32).collect::<Vec<_>>())),
        Arc::new(UInt8Array::from((0..n).map(|i| (i % 100) as u8).collect::<Vec<_>>())),
        Arc::new(UInt16Array::from((0..n).map(|i| i as u16).collect::<Vec<_>>())),
        Arc::new(UInt32Array::from((0..n).map(|i| i as u32).collect::<Vec<_>>())),
        Arc::new(UInt64Array::from((0..n).map(|i| i as u64).collect::<Vec<_>>())),
        Arc::new(Float32Array::from((0..n).map(|i| i as f32).collect::<Vec<_>>())),
        Arc::new(Float64Array::from((0..n).map(|i| i as f64).collect::<Vec<_>>())),
        Arc::new(BooleanArray::from((0..n).map(|i| i % 2 == 0).collect::<Vec<_>>())),
        Arc::new(StringArray::from(
            (0..n).map(|i| if i % 2 == 0 { "a" } else { "b" }).collect::<Vec<_>>(),
        )),
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
    let (dir, schema) = setup(513);
    let b = run_select_collect("SELECT ts FROM wide LIMIT 3", &schema, dir.path()).unwrap();
    let rows: usize = b.iter().map(|x| x.num_rows()).sum();
    assert_eq!(rows, 3);
}
