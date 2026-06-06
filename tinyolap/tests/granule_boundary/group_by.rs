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

use std::collections::HashMap;

fn check_n(n: usize) {
    let (dir, schema) = setup(n);
    let b = run_select_collect("SELECT label, COUNT(ts) FROM wide GROUP BY label", &schema, dir.path()).unwrap();
    let batch = &b[0];
    let keys = batch.column(0).as_any().downcast_ref::<StringArray>().unwrap();
    let counts = batch.column(1).as_any().downcast_ref::<UInt64Array>().unwrap();
    let mut got = HashMap::new();
    for i in 0..batch.num_rows() {
        got.insert(keys.value(i).to_string(), counts.value(i));
    }
    // a = even i, b = odd i
    let expected: HashMap<String, u64> =
        [("a".to_string(), (n as u64 + 1) / 2), ("b".to_string(), n as u64 / 2)]
            .into_iter()
            .collect();
    assert_eq!(got, expected, "group by label n={n}");
}

#[test]
fn boundary_511() {
    check_n(511);
}

#[test]
fn boundary_512() {
    check_n(512);
}

#[test]
fn boundary_513() {
    check_n(513);
}
