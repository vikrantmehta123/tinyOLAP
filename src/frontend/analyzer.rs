//! Schema validation — checks that all names and types in a query are valid
//! against the known table schema. Runs after the shape validator; does not check
//! SQL structure, only that referenced tables and columns actually exist.

use crate::catalog::schema::{DataType, TableSchema};
use core::ops::ControlFlow;
use sqlparser::ast::{Expr, Statement, TableFactor, Value, ValueWithSpan, Visit, Visitor};

struct AnalyzerVisitor<'a> {
    schema: &'a TableSchema,
    error: Option<String>,
}

impl<'a> AnalyzerVisitor<'a> {
    fn resolve_column(&self, name: &str) -> Result<&crate::catalog::schema::ColumnSchema, String> {
        self.schema
            .columns
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| format!("unknown column: {}", name))
    }
    fn check_type_compat(&self, data_type: &DataType, val: &ValueWithSpan) -> Result<(), String> {
        let col_is_numeric = matches!(
            data_type,
            DataType::I8
                | DataType::I16
                | DataType::I32
                | DataType::I64
                | DataType::U8
                | DataType::U16
                | DataType::U32
                | DataType::U64
                | DataType::F32
                | DataType::F64
        );
        match &val.value {
            Value::Number(_, _) if !col_is_numeric => {
                Err("type mismatch: numeric literal compared to non-numeric column".to_string())
            }
            Value::SingleQuotedString(_) if col_is_numeric => {
                Err("type mismatch: string literal compared to numeric column".to_string())
            }
            _ => Ok(()),
        }
    }
}

impl<'a> Visitor for AnalyzerVisitor<'a> {
    type Break = ();

    fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<()> {
        if let TableFactor::Table { name, .. } = table {
            let table_name = name.to_string();
            if table_name != self.schema.name {
                self.error = Some(format!("Unknown table: {}", table_name));
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        match expr {
            Expr::Identifier(ident) => {
                if let Err(e) = self.resolve_column(&ident.value) {
                    self.error = Some(e);
                    return ControlFlow::Break(());
                }
            }
            Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                let table_name = &parts[0].value;
                let col_name = &parts[1].value;
                if table_name != &self.schema.name {
                    self.error = Some(format!(
                        "unknown table in qualified reference: {}",
                        table_name
                    ));
                    return ControlFlow::Break(());
                }
                if let Err(e) = self.resolve_column(col_name) {
                    self.error = Some(e);
                    return ControlFlow::Break(());
                }
            }
            // Broad group check: numeric columns accept numeric literals, string
            // columns accept string literals. Mixing the two groups is rejected.
            // We don't distinguish i32 from i64 at the SQL layer — that would
            // surprise users writing plain integer literals.
            Expr::BinaryOp { left, right, .. } => {
                if let (Expr::Identifier(ident), Expr::Value(val)) = (left.as_ref(), right.as_ref())
                {
                    if let Ok(col) = self.resolve_column(&ident.value) {
                        if let Err(e) = self.check_type_compat(&col.data_type, val) {
                            self.error = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

pub fn analyze(stmt: &Statement, schema: &TableSchema) -> Result<(), String> {
    match stmt {
        Statement::Insert(_) => Ok(()),
        Statement::Query(_) => {
            let mut visitor = AnalyzerVisitor {
                schema,
                error: None,
            };
            stmt.visit(&mut visitor);
            match visitor.error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }
        _ => Err("unsupported statement type".to_string()),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};
    use crate::frontend::parser::parse;

    fn make_schema() -> TableSchema {
        TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema { name: "id".to_string(),   data_type: DataType::I64 },
                ColumnSchema { name: "name".to_string(), data_type: DataType::Str },
                ColumnSchema { name: "age".to_string(),  data_type: DataType::I32 },
            ],
            sort_key: vec![0]
        }
    }

    fn analyze_sql(sql: &str) -> Result<(), String> {
        let stmt = parse(sql).unwrap();
        analyze(&stmt, &make_schema())
    }

    #[test]
    fn valid_column_reference_passes() {
        assert!(analyze_sql("SELECT id, name FROM users").is_ok());
    }

    #[test]
    fn select_star_passes() {
        assert!(analyze_sql("SELECT * FROM users").is_ok());
    }

    #[test]
    fn unknown_column_is_rejected() {
        assert!(analyze_sql("SELECT unknown_col FROM users").is_err());
    }

    #[test]
    fn unknown_table_is_rejected() {
        assert!(analyze_sql("SELECT id FROM orders").is_err());
    }

     #[test]
    fn qualified_column_wrong_table_is_rejected() {
        assert!(analyze_sql("SELECT orders.id FROM users").is_err());
    }

    #[test]
    fn qualified_column_valid_passes() {
        assert!(analyze_sql("SELECT users.id FROM users").is_ok());
    }

    #[test]
    fn type_mismatch_string_to_numeric_is_rejected() {
        assert!(analyze_sql("SELECT id FROM users WHERE age = 'hello'").is_err());
    }    
    
    #[test]
    fn type_mismatch_numeric_to_string_is_rejected() {
        assert!(analyze_sql("SELECT id FROM users WHERE name = 42").is_err());
    }

    #[test]
    fn valid_type_comparison_passes() {
        assert!(analyze_sql("SELECT id FROM users WHERE age > 30").is_ok());
    }

}