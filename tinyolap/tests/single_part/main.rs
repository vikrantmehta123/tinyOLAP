// One integration-test binary for queries over a single-part table.
// Each concern is a submodule; shared setup lives in tests/common/mod.rs.

#[path = "../common/mod.rs"]
mod common;

mod aggregates;
mod projection;
mod filter;
mod group_by;
mod limit;
