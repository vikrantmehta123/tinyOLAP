use arrow::array::Int64Array;
use tinyolap::run_select_collect;

use crate::common::setup_basic_table;

/// Run a query that projects only `n` and collect its values in row order.
fn select_n(sql: &str) -> Vec<i64> {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect(sql, &schema, dir.path()).unwrap();
    let mut out = Vec::new();
    for batch in &batches {
        let arr = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        out.extend(arr.values().iter().copied());
    }
    out
}

#[test]
fn filter_greater_than() {
    assert_eq!(select_n("SELECT n FROM basic WHERE n > 3"), vec![4, 5, 6]);
}

#[test]
fn filter_equals() {
    assert_eq!(select_n("SELECT n FROM basic WHERE n = 3"), vec![3]);
}

#[test]
fn filter_not_equals() {
    assert_eq!(select_n("SELECT n FROM basic WHERE n != 3"), vec![1, 2, 4, 5, 6]);
}

#[test]
fn filter_and() {
    assert_eq!(select_n("SELECT n FROM basic WHERE n >= 4 AND n <= 5"), vec![4, 5]);
}

#[test]
fn filter_or() {
    assert_eq!(select_n("SELECT n FROM basic WHERE n = 1 OR n = 6"), vec![1, 6]);
}

#[test]
fn filter_matches_nothing() {
    // No aggregate above the filter, so an empty match is simply zero rows.
    assert_eq!(select_n("SELECT n FROM basic WHERE n > 100"), Vec::<i64>::new());
}
