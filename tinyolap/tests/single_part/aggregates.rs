// tests/one_part_aggregates.rs
//
// Aggregate-query correctness against a tiny, hand-verifiable single-part table.
//   n     : I64  -> [1, 2, 3, 4, 5, 6]   (sum 21, min 1, max 6, count 6, avg 3.5)
//   price : F64  -> [10, 20, 30, 40, 50, 60]   (sum 210, max 60)


use arrow::array::{Float64Array, Int64Array, UInt64Array};
use tinyolap::run_select_collect;

use crate::common::setup_basic_table;

#[test]
fn sum_of_i64_column() {
    let (dir, schema) = setup_basic_table();
    let batches =
        run_select_collect("SELECT SUM(n) FROM basic", &schema, dir.path()).expect("query failed");

    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 1);

    // SUM over i8/i16/i32/i64 widens to Int64.
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("SUM(n) should be Int64");
    assert_eq!(col.value(0), 21);
}

#[test]
fn sum_of_f64_column() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT SUM(price) FROM basic", &schema, dir.path())
        .expect("query failed");

    let batch = &batches[0];
    assert_eq!(batch.num_rows(), 1);
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<Float64Array>()
        .expect("SUM(price) should be Float64");
    // 210.0 is exactly representable in f64, so == is safe here.
    assert_eq!(col.value(0), 210.0);
}

#[test]
fn count_of_column() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT COUNT(n) FROM basic", &schema, dir.path())
        .expect("query failed");

    let batch = &batches[0];
    assert_eq!(batch.num_rows(), 1);
    // COUNT always returns UInt64.
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .expect("COUNT(n) should be UInt64");
    assert_eq!(col.value(0), 6);
}

#[test]
fn min_of_column() {
    let (dir, schema) = setup_basic_table();
    let batches =
        run_select_collect("SELECT MIN(n) FROM basic", &schema, dir.path()).expect("query failed");

    let batch = &batches[0];
    // MIN preserves the input type -> Int64 for an i64 column.
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("MIN(n) should be Int64");
    assert_eq!(col.value(0), 1);
}

#[test]
fn max_of_column() {
    let (dir, schema) = setup_basic_table();
    let batches =
        run_select_collect("SELECT MAX(n) FROM basic", &schema, dir.path()).expect("query failed");

    let batch = &batches[0];
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .expect("MAX(n) should be Int64");
    assert_eq!(col.value(0), 6);
}

#[test]
fn avg_of_column() {
    let (dir, schema) = setup_basic_table();
    let batches =
        run_select_collect("SELECT AVG(n) FROM basic", &schema, dir.path()).expect("query failed");

    let batch = &batches[0];
    // AVG always returns Float64. 21/6 = 3.5, exactly representable.
    let col = batch
        .column(0)
        .as_any()
        .downcast_ref::<Float64Array>()
        .expect("AVG(n) should be Float64");
    assert_eq!(col.value(0), 3.5);
}

#[test]
fn multiple_aggregates_in_one_select() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect(
        "SELECT SUM(price), AVG(n), MIN(n), MAX(price), COUNT(n) FROM basic",
        &schema,
        dir.path(),
    )
    .expect("query failed");

    let batch = &batches[0];
    assert_eq!(batch.num_rows(), 1);
    assert_eq!(batch.num_columns(), 5, "one column per projected aggregate");

    // Columns come out in projection order.
    let sum_price = batch.column(0).as_any().downcast_ref::<Float64Array>().unwrap();
    let avg_n = batch.column(1).as_any().downcast_ref::<Float64Array>().unwrap();
    let min_n = batch.column(2).as_any().downcast_ref::<Int64Array>().unwrap();
    let max_price = batch.column(3).as_any().downcast_ref::<Float64Array>().unwrap();
    let count_n = batch.column(4).as_any().downcast_ref::<UInt64Array>().unwrap();

    assert_eq!(sum_price.value(0), 210.0);
    assert_eq!(avg_n.value(0), 3.5);
    assert_eq!(min_n.value(0), 1);
    assert_eq!(max_price.value(0), 60.0);
    assert_eq!(count_n.value(0), 6);
}
