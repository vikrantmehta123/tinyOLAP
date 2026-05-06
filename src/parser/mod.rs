//! SQL parsing for tinyOLAP.
//
//! Tokenising and grammar is handled by sqlparser-rs. 
//! The crate gives us an AST for free; we only need to
//! lower its generic representation to the small subset tinyOLAP actually
//! supports (see lower.rs).

pub mod ast;
mod lower;

pub use ast::{Statement, InsertStmt, Literal};

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser as SqlParser;

#[derive(Debug)]
pub enum ParseError {
    Syntax(String),
    Unsupported(String),
    Empty,
    MultipleStatements, // We don't execute multiple SQL statements. 
}

pub fn parse(sql: &str) -> Result<Statement, ParseError> {
    let dialect = GenericDialect {};
    let mut stmts = SqlParser::parse_sql(&dialect, sql)
        .map_err(|e| ParseError::Syntax(e.to_string()))?;
    match stmts.len() {
        0 => Err(ParseError::Empty),
        1 => lower::lower(stmts.pop().unwrap()),
        _ => Err(ParseError::MultipleStatements),
    }
}
