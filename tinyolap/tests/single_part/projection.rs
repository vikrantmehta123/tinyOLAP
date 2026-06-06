use arrow::array::{Float64Array, Int64Array, RecordBatch};
use tinyolap::run_select_collect;

use crate::common::setup_basic_table;

/// Concatenate an Int64 column across all output batches.
fn collect_i64(batches: &[RecordBatch], col: usize) -> Vec<i64> {
    let mut out = Vec::new();
    for batch in batches {
        let arr = batch.column(col).as_any().downcast_ref::<Int64Array>().unwrap();
        out.extend(arr.values().iter().copied());
    }
    out
}

#[test]
fn select_one_column() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT n FROM basic", &schema, dir.path()).unwrap();
    assert_eq!(collect_i64(&batches, 0), vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn select_two_columns() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT n, price FROM basic", &schema, dir.path()).unwrap();

    assert_eq!(batches[0].num_columns(), 2);
    assert_eq!(collect_i64(&batches, 0), vec![1, 2, 3, 4, 5, 6]); // col order = SELECT order

    let mut prices = Vec::new();
    for batch in &batches {
        let arr = batch.column(1).as_any().downcast_ref::<Float64Array>().unwrap();
        prices.extend(arr.values().iter().copied());
    }
    assert_eq!(prices, vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
}

#[test]
fn select_all_columns() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT * FROM basic", &schema, dir.path()).unwrap();

    // SELECT * expands to schema order: n (col 0), price (col 1).
    assert_eq!(batches[0].num_columns(), 2);
    assert_eq!(collect_i64(&batches, 0), vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn select_columns_reversed() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT price, n FROM basic", &schema, dir.path()).unwrap();

    // Projection order is honored: price -> col 0, n -> col 1.
    let prices = batches[0].column(0).as_any().downcast_ref::<Float64Array>().unwrap();
    assert_eq!(prices.value(0), 10.0);
    assert_eq!(collect_i64(&batches, 1), vec![1, 2, 3, 4, 5, 6]);
}
