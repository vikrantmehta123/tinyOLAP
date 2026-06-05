//! Exec Operators
//!
//! These operators drive the actual execution of the query

pub mod aggregation;
pub mod builder;
pub mod executor;
pub mod expr;
pub mod filter;
pub mod full_scan;
pub mod gather;
pub mod limit;
pub mod project;
pub mod work_source;
pub mod zone_map_scan;
