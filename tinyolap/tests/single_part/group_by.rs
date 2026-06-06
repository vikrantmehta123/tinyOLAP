use std::collections::HashMap;

use arrow::array::{Float64Array, Int64Array};
use tinyolap::run_select_collect;

use crate::common::setup_basic_table;

#[test]
fn group_by_all_distinct_keys() {
    let (dir, schema) = setup_basic_table();
    let batches =
        run_select_collect("SELECT n, SUM(price) FROM basic GROUP BY n", &schema, dir.path())
            .unwrap();

    // GROUP BY row order is NOT guaranteed (hash-map iteration order), so
    // compare as a key -> value map, never positionally.
    let batch = &batches[0];
    let keys = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
    let sums = batch.column(1).as_any().downcast_ref::<Float64Array>().unwrap();

    let mut got = HashMap::new();
    for i in 0..batch.num_rows() {
        got.insert(keys.value(i), sums.value(i));
    }

    // Every key is distinct here, so each group's SUM is just that row's price.
    let expected: HashMap<i64, f64> =
        [(1, 10.0), (2, 20.0), (3, 30.0), (4, 40.0), (5, 50.0), (6, 60.0)]
            .into_iter()
            .collect();
    assert_eq!(got, expected);
}
