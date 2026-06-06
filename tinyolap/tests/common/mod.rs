//! Setup for tinyOLAP Integration Tests

use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Array, Int64Array, RecordBatch};
use arrow::datatypes::{Field, Schema};
use tempfile::TempDir;

use tinyolap::catalog::schema::{ColumnSchema, DataType, TableSchema};
use tinyolap::storage::table_writer::TableWriter;

/// Build a tiny two-column table and write a single
/// part of six rows into it.
///
///   n     : I64  -> [1, 2, 3, 4, 5, 6]
///   price : F64  -> [10.0, 20.0, 30.0, 40.0, 50.0, 60.0]
/// 
/// Basic single part functionality is tested with this
pub fn setup_basic_table() -> (TempDir, TableSchema) {
    let schema = TableSchema {
        name: "basic".to_string(),
        columns: vec![
            ColumnSchema { name: "n".to_string(),     data_type: DataType::I64 },
            ColumnSchema { name: "price".to_string(), data_type: DataType::F64 },
        ],
        sort_key: vec![0],
    };

    let dir = TempDir::new().expect("failed to create temp dir");
    TableSchema::create(dir.path(), &schema).expect("failed to write schema.json");

    // One RecordBatch == one part.
    let n: ArrayRef = Arc::new(Int64Array::from(vec![1, 2, 3, 4, 5, 6]));
    let price: ArrayRef =
        Arc::new(Float64Array::from(vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]));

    let fields: Vec<Field> = schema
        .columns
        .iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));
    let batch =
        RecordBatch::try_new(arrow_schema, vec![n, price]).expect("failed to build RecordBatch");

    let writer = TableWriter::open(dir.path().to_path_buf()).expect("failed to open TableWriter");
    writer.insert(batch).expect("insert failed");

    (dir, schema)
}
