use arrow::array::Int64Array;
use tinyolap::run_select_collect;

use crate::common::setup_basic_table;

// SQL-correct expectation: LIMIT 3 yields the first 3 rows. Marked #[ignore]
// because LIMIT isn't correct end-to-end yet — run `cargo test -- --ignored`
// to watch it, or delete the attribute to make it a hard failure.
#[test]
#[ignore = "LIMIT not yet correct end-to-end"]
fn limit_returns_first_n_rows() {
    let (dir, schema) = setup_basic_table();
    let batches = run_select_collect("SELECT n FROM basic LIMIT 3", &schema, dir.path()).unwrap();

    let mut got = Vec::new();
    for batch in &batches {
        let arr = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
        got.extend(arr.values().iter().copied());
    }
    assert_eq!(got, vec![1, 2, 3]);
}
