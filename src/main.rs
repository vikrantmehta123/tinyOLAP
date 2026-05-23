mod catalog;
mod cli;
mod config;
mod encoding;
mod frontend;
mod logical_plan;
mod physical_plan;
mod storage;
mod execution;

use std::path::PathBuf;
use catalog::schema::TableSchema;

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
        eprintln!("execution not yet wired (TASK-002 subtask 11): got {sql:?}");
    }
}
