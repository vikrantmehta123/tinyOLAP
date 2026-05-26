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
    fn resolve_column_expr(&self, expr:&Expr) -> Result<Option<&crate::catalog::schema::ColumnSchema>, String> {
        match expr {
            Expr::Identifier(ident) => Ok(Some(self.resolve_column(&ident.value)?)),
            Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                let table_name = &parts[0].value;
                let col_name = &parts[1].value;

                if table_name != &self.schema.name {
                    return Err(format!("unknown table in qualified reference: {}", table_name));
                }

                Ok(Some(self.resolve_column(col_name)?))
            }

            _ => Ok(None),
        }
    }

    fn literal_expr<'b>(&self, expr: &'b Expr) -> Option<&'b ValueWithSpan> {
        match expr {
            Expr::Value(value) => Some(value),
            _ => None,
        }
    }

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
            // Check range compatibility of the type. For instance, a value like 40_000 shouldn't be accepted for I8
            Value::Number(s, _) => {
                let (min, max): (i128, i128) = match data_type {
                    DataType::I8  => (i8::MIN  as i128, i8::MAX  as i128),
                    DataType::I16 => (i16::MIN as i128, i16::MAX as i128),
                    DataType::I32 => (i32::MIN as i128, i32::MAX as i128),
                    DataType::I64 => (i64::MIN as i128, i64::MAX as i128),
                    DataType::U8  => (0, u8::MAX  as i128),
                    DataType::U16 => (0, u16::MAX as i128),
                    DataType::U32 => (0, u32::MAX as i128),
                    DataType::U64 => (0, u64::MAX as i128),
                    DataType::F32 | DataType::F64 => return Ok(()),
                    _ => {
                        return Err(format!(
                            "type mismatch: numeric literal compared to {:?} column",
                            data_type
                        ));
                    }
                };

                let n = s.parse::<i128>().map_err(|_| {
                    format!("type mismatch: non-integer literal {} compared to integer column", s)
                })?;
                if n < min || n > max {
                    return Err(format!(
                        "literal {} is out of range for column type {:?} ({}..={})",
                        s, data_type, min, max
                    ));
                }

                Ok(())
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
            Expr::BinaryOp { left, right, .. } => {
                // We can either get (literal, column) or (column, literal). Two matches for that.
                match (
                    self.resolve_column_expr(left.as_ref()),
                    self.literal_expr(right.as_ref()),
                ) {
                    (Ok(Some(col)), Some(val)) => {
                        if let Err(e) = self.check_type_compat(&col.data_type, val) {
                            self.error = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                    (Err(e), _) => {
                        self.error = Some(e);
                        return ControlFlow::Break(());
                    }
                    _ => {}
                }

                match (
                    self.literal_expr(left.as_ref()),
                    self.resolve_column_expr(right.as_ref()),
                ) {
                    (Some(val), Ok(Some(col))) => {
                        if let Err(e) = self.check_type_compat(&col.data_type, val) {
                            self.error = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                    (_, Err(e)) => {
                        self.error = Some(e);
                        return ControlFlow::Break(());
                    }
                    _ => {}
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

    #[test]
    fn i16_overflow_is_rejected() {
        // 40000 exceeds i16::MAX (32767)
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema { name: "country_id".to_string(), data_type: DataType::I16 },
            ],
            sort_key: vec![0],
        };
        let stmt = parse("SELECT country_id FROM users WHERE country_id = 40000").unwrap();
        assert!(analyze(&stmt, &schema).is_err());
    }

    #[test]
    fn i16_in_range_passes() {
        let schema = TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema { name: "country_id".to_string(), data_type: DataType::I16 },
            ],
            sort_key: vec![0],
        };
        let stmt = parse("SELECT country_id FROM users WHERE country_id = 100").unwrap();
        assert!(analyze(&stmt, &schema).is_ok());
    }
}
