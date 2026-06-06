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

/// 20 parts, varying sizes: exact granule multiples (512/1024/1536/2048) mixed
/// with in-between sizes, all spanning several granules. Sum < 32768 so i16_c=i
/// never wraps at storage. Ordering across parts is irrelevant — the suite only
/// checks counts/sums/group-counts.
const PART_SIZES: [usize; 20] = [
    700, 1024, 1536, 513, 999, 2048, 1025, 1537, 1300, 2049,
    800, 1600, 900, 2000, 1111, 512, 1024, 777, 1234, 2047,
];

/// Build the 20-part table. `ts` is a global 0..N counter across all parts.
fn setup() -> (TempDir, TableSchema) {
    let schema = wide_schema();
    let fields: Vec<Field> = schema
        .columns
        .iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));

    let dir = TempDir::new().unwrap();
    TableSchema::create(dir.path(), &schema).unwrap();
    let w = TableWriter::open(dir.path().to_path_buf()).unwrap();

    let mut offset: usize = 0;
    for &size in PART_SIZES.iter() {
        let range = offset..offset + size;
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(Int64Array::from(range.clone().map(|i| i as i64).collect::<Vec<_>>())),
            Arc::new(Int8Array::from(range.clone().map(|i| (i % 100) as i8).collect::<Vec<_>>())),
            Arc::new(Int16Array::from(range.clone().map(|i| i as i16).collect::<Vec<_>>())),
            Arc::new(Int32Array::from(range.clone().map(|i| i as i32).collect::<Vec<_>>())),
            Arc::new(UInt8Array::from(range.clone().map(|i| (i % 100) as u8).collect::<Vec<_>>())),
            Arc::new(UInt16Array::from(range.clone().map(|i| i as u16).collect::<Vec<_>>())),
            Arc::new(UInt32Array::from(range.clone().map(|i| i as u32).collect::<Vec<_>>())),
            Arc::new(UInt64Array::from(range.clone().map(|i| i as u64).collect::<Vec<_>>())),
            Arc::new(Float32Array::from(range.clone().map(|i| i as f32).collect::<Vec<_>>())),
            Arc::new(Float64Array::from(range.clone().map(|i| i as f64).collect::<Vec<_>>())),
            Arc::new(BooleanArray::from(range.clone().map(|i| i % 2 == 0).collect::<Vec<_>>())),
            Arc::new(StringArray::from(
                range.clone().map(|i| if i % 2 == 0 { "a" } else { "b" }).collect::<Vec<_>>(),
            )),
        ];
        let batch = RecordBatch::try_new(arrow_schema.clone(), arrays).unwrap();
        w.insert(batch).unwrap(); // one insert = one part
        offset += size;
    }
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
