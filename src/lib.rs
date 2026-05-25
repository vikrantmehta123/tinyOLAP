pub mod catalog;
pub mod config;
pub mod dml;
pub mod encoding;
pub mod execution;
pub mod frontend;
pub mod logical_plan;
pub mod physical_plan;
pub mod storage;

use std::path::Path;

use arrow::array::RecordBatch;

use crate::catalog::schema::TableSchema;
use crate::execution::builder::build;
use crate::frontend::analyzer::analyze;
use crate::frontend::parser::{Statement, parse};
use crate::frontend::validator::validate;

/// Plan and execute a SELECT statement, collecting every output batch.
///
/// Mirrors the SELECT path of the REPL (`src/main.rs::run_select`) but returns
/// the materialized batches instead of pretty-printing them — exactly the shape
/// a benchmark or external driver wants.
///
/// INSERTs go through `dml::insert_builder` + `TableWriter::insert`; this helper
/// is SELECT-only and rejects INSERT explicitly.
pub fn run_select_collect(
    sql: &str,
    schema: &TableSchema,
    table_dir: &Path,
) -> Result<Vec<RecordBatch>, String> {
    let stmt = parse(sql)?;
    validate(&stmt)?;
    analyze(&stmt, schema)?;

    if matches!(stmt, Statement::Insert(_)) {
        return Err("run_select_collect: INSERT not supported".into());
    }

    let mut logical_plan = logical_plan::lower::lower(&stmt, schema)?;
    logical_plan = logical_plan::optimizer::Optimizer::new().optimize(logical_plan);

    let physical_plan = physical_plan::lower::lower(logical_plan);

    let mut plan = build(physical_plan, schema, table_dir).map_err(|e| e.to_string())?;

    let mut batches: Vec<RecordBatch> = Vec::new();
    loop {
        match plan.next_batch() {
            None => break,
            Some(Ok(batch)) => batches.push(batch),
            Some(Err(e)) => return Err(e.to_string()),
        }
    }
    Ok(batches)
}
