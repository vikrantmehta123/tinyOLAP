mod catalog;
mod cli;
mod config;
mod encoding;
mod execution;
mod frontend;
mod logical_plan;
mod physical_plan;
mod storage;
mod dml;

use arrow::array::RecordBatch;
use catalog::schema::TableSchema;
use frontend::parser::Statement;
use std::path::{Path, PathBuf};
use storage::table_writer::TableWriter;

use crate::{
    execution::builder::build,
    frontend::{analyzer::analyze, parser::parse, validator::validate},
};

fn run_sql(sql: &str, schema: &TableSchema, table_dir: &Path) -> Result<(), String> {
    let stmt = parse(sql)?;
    validate(&stmt)?;
    analyze(&stmt, schema)?;

    // Dispatch on statement kind. INSERT and SELECT take very different paths
    // — INSERT skips planning entirely (just build a RecordBatch and hand it
    // to the writer).
    if matches!(stmt, Statement::Insert(_)) {
        return run_insert(&stmt, schema, table_dir);
    }
    run_select(&stmt, schema, table_dir)
}

fn run_insert(
    stmt: &Statement,
    schema: &TableSchema,
    table_dir: &Path,
) -> Result<(), String> {
    let batch = dml::insert_builder::build_record_batch(stmt, schema)?;

    let writer = TableWriter::open(table_dir.to_path_buf())
        .map_err(|e| e.to_string())?;
    let meta = writer.insert(batch).map_err(|e| e.to_string())?;

    println!("OK ({} rows inserted, part_{:05})", meta.rows, meta.part_id);
    Ok(())
}

fn run_select(
    stmt: &Statement,
    schema: &TableSchema,
    table_dir: &Path,
) -> Result<(), String> {
    let mut logical_plan = logical_plan::lower::lower(stmt, schema)?;
    logical_plan = logical_plan::optimizer::Optimizer::new(&schema).optimize(logical_plan);

    let physical_plan = physical_plan::lower::lower(logical_plan);
    // TODO(TASK-004): re-enable physical optimizer once ZoneMapScanExec lands.
    
    println!("{}", physical_plan);


    let mut plan = build(physical_plan, schema, table_dir)
        .map_err(|e| e.to_string())?;

    println!("{}", plan);


    let mut batches: Vec<RecordBatch> = Vec::new();
    loop {
        match plan.next_batch() {
            None             => break,
            Some(Ok(batch))  => batches.push(batch),
            Some(Err(e))     => return Err(e.to_string()),
        }
    }

    if batches.is_empty() {
        println!("(0 rows)");
    } else {
        arrow::util::pretty::print_batches(&batches).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
    let table_dir = PathBuf::from(config::DATA_DIR);
    std::fs::create_dir_all(&table_dir).expect("failed to create data dir");

    let schema = TableSchema::open(&table_dir).unwrap_or_else(|_| {
        eprintln!("No schema.json found in {:?}. Create one first.", table_dir);
        std::process::exit(1);
    });

    println!("tinyOLAP ready. Table: '{}'", schema.name);
    println!("Type SQL and press Enter. Ctrl-C or Ctrl-D to quit.\n");

    let mut repl = cli::Repl::new().expect("failed to init CLI");
    loop {
        let Some(sql) = repl.next_line("> ") else {
            break;
        };
        if sql.is_empty() {
            continue;
        }
        if let Err(e) = run_sql(&sql, &schema, &table_dir) {
            eprintln!("error: {e}");
        }
    }
}
