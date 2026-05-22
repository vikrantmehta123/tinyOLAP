mod aggregator;
mod cli;
mod config;
mod encoding;
mod executor;
mod parser;
mod storage;
mod analyser;
mod evaluator;
mod processors;
mod frontend;

use std::path::PathBuf;
use storage::schema::TableDef;

fn main() {
    let table_dir = PathBuf::from(config::DATA_DIR);
    std::fs::create_dir_all(&table_dir).expect("failed to create data dir");

    let schema = TableDef::open(&table_dir).unwrap_or_else(|_| {
        eprintln!("No schema.json found in {:?}. Create one first.", table_dir);
        std::process::exit(1);
    });

    println!("tinyOLAP ready. Table: '{}'", schema.name);
    println!("Type SQL and press Enter. Ctrl-C or Ctrl-D to quit.\n");

    let mut repl = cli::Repl::new().expect("failed to init CLI");
    loop {
        let Some(sql) = repl.next_line("> ") else { break };
        if sql.is_empty() {
            continue;
        }

        match crate::parser::parse(&sql) {
            Err(e) => eprintln!("parse error: {e:?}"),
            Ok(stmt) => {
                println!("lowered AST: {stmt:?}");
                match stmt {
                    crate::parser::Statement::Insert(insert_stmt) => {
                        match executor::execute_insert(insert_stmt, &schema, table_dir.clone()) {
                            Ok(meta) => {
                                println!("OK ({} rows inserted, part_{})", meta.rows, meta.part_id);
                            }
                            Err(e) => eprintln!("error: {e}"),
                        }
                    }
                    crate::parser::Statement::Select(select_stmt) => {
                        match executor::execute_select(select_stmt, &schema, table_dir.clone()) {
                            Ok(chunks) => println!("{:?}", chunks),
                            Err(e) => eprintln!("error: {e}"),
                        }
                    }
                }
            }
        }
    }
}
