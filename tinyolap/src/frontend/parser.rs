//! Thin wrapper around sqlparser. Parses a SQL string into a single sqlparser
//! AST Statement. All dialect handling is delegated to sqlparser; no logic lives here.

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
pub use sqlparser::ast::Statement;

pub fn parse(sql: &str) -> Result<Statement, String> {
    let dialect = GenericDialect {};
    let statements = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;
    statements.into_iter().next().ok_or_else(|| "empty input".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_select() {
        let result = parse("SELECT id from users");
        assert!(result.is_ok());
    }

    #[test]
    fn parses_valid_insert() {
        let result = parse("INSERT INTO users VALUES (1, 'alice')");
        assert!(result.is_ok())
    }

    #[test]
    fn rejects_empty_input() {
        let result = parse("");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_sql(){
        let result = parse("THIS IS NOT AN SQL QUERY");
        assert!(result.is_err());
    }
}