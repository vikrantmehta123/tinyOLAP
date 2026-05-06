//! Lowers sqlparser's API to tinyOLAP's types
//! Only INSERT INTO and SELECT statements are supported by tinyOLAP. 
//! Only these statements are lowered. 
//! Everything else throws an exception.
//! 
//! NOTE: This module is mostly vibecoded.

use crate::parser::ast::{AggFunc, CmpOp, Predicate, Projection, SelectExpr, SelectStmt};
use crate::parser::{InsertStmt, Literal, ParseError, Statement};
use sqlparser::ast as s;

pub fn lower(stmt: s::Statement) -> Result<Statement, ParseError> {
    match stmt {
        s::Statement::Insert(ins) => lower_insert(ins).map(Statement::Insert),
        s::Statement::Query(q) => lower_select(*q).map(Statement::Select),
        other => Err(ParseError::Unsupported(format!("statement: {:?}", other))),
    }
}

fn lower_insert(ins: s::Insert) -> Result<InsertStmt, ParseError> {
    let table = ins.table.to_string();
    let columns = if ins.columns.is_empty() {
        None
    } else {
        Some(ins.columns.into_iter().map(|c| c.value).collect())
    };
    let source = ins
        .source
        .ok_or_else(|| ParseError::Unsupported("INSERT without VALUES".into()))?;
    let rows = match *source.body {
        s::SetExpr::Values(s::Values { rows, .. }) => rows
            .into_iter()
            .map(|row| row.into_iter().map(lower_expr).collect())
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(ParseError::Unsupported("INSERT must use VALUES".into())),
    };
    Ok(InsertStmt {
        table,
        columns,
        rows,
    })
}

fn lower_expr(e: s::Expr) -> Result<Literal, ParseError> {
    match e {
        s::Expr::Value(v) => lower_value(v.value),
        s::Expr::UnaryOp {
            op: s::UnaryOperator::Minus,
            expr,
        } => match lower_expr(*expr)? {
            Literal::Int(i) => Ok(Literal::Int(-i)),
            Literal::Float(f) => Ok(Literal::Float(-f)),
            Literal::UInt(u) => Ok(Literal::Int(-(u as i64))),
            _ => Err(ParseError::Unsupported("unary minus on non-numeric".into())),
        },
        other => Err(ParseError::Unsupported(format!(
            "expr in VALUES: {:?}",
            other
        ))),
    }
}

fn lower_value(v: s::Value) -> Result<Literal, ParseError> {
    match v {
        s::Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(Literal::Int(i))
            } else if let Ok(u) = n.parse::<u64>() {
                Ok(Literal::UInt(u))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(Literal::Float(f))
            } else {
                Err(ParseError::Syntax(format!("bad number: {n}")))
            }
        }
        s::Value::SingleQuotedString(s) => Ok(Literal::Str(s)),
        s::Value::Boolean(b) => Ok(Literal::Bool(b)),
        s::Value::Null => Ok(Literal::Null),
        other => Err(ParseError::Unsupported(format!("literal: {:?}", other))),
    }
}

fn lower_cmpop(op: s::BinaryOperator) -> Result<CmpOp, ParseError> {
    match op {
        s::BinaryOperator::Eq => Ok(CmpOp::Eq),
        s::BinaryOperator::NotEq => Ok(CmpOp::Ne),
        s::BinaryOperator::Lt => Ok(CmpOp::Lt),
        s::BinaryOperator::LtEq => Ok(CmpOp::Le),
        s::BinaryOperator::Gt => Ok(CmpOp::Gt),
        s::BinaryOperator::GtEq => Ok(CmpOp::Ge),
        other => Err(ParseError::Unsupported(format!(
            "comparison op: {:?}",
            other
        ))),
    }
}

fn lower_predicate(e: s::Expr) -> Result<Predicate, ParseError> {
    match e {
        s::Expr::BinaryOp {
            left,
            op: s::BinaryOperator::And,
            right,
        } => Ok(Predicate::And(
            Box::new(lower_predicate(*left)?),
            Box::new(lower_predicate(*right)?),
        )),
        s::Expr::BinaryOp {
            left,
            op: s::BinaryOperator::Or,
            right,
        } => Ok(Predicate::Or(
            Box::new(lower_predicate(*left)?),
            Box::new(lower_predicate(*right)?),
        )),
        s::Expr::UnaryOp {
            op: s::UnaryOperator::Not,
            expr,
        } => Ok(Predicate::Not(Box::new(lower_predicate(*expr)?))),
        s::Expr::BinaryOp { left, op, right } => {
            let col = match *left {
                s::Expr::Identifier(id) => id.value,
                other => {
                    return Err(ParseError::Unsupported(format!(
                        "LHS must be a column name, got: {:?}",
                        other
                    )));
                }
            };
            let value = match *right {
                s::Expr::Value(v) => lower_value(v.value)?,
                other => {
                    return Err(ParseError::Unsupported(format!(
                        "RHS must be a literal, got: {:?}",
                        other
                    )));
                }
            };
            Ok(Predicate::Cmp {
                col,
                op: lower_cmpop(op)?,
                value,
            })
        }
        other => Err(ParseError::Unsupported(format!(
            "unsupported predicate: {:?}",
            other
        ))),
    }
}

fn lower_agg_func(name: &str) -> Result<AggFunc, ParseError> {
    match name.to_lowercase().as_str() {
        "sum" => Ok(AggFunc::Sum),
        "max" => Ok(AggFunc::Max),
        "min" => Ok(AggFunc::Min),
        "count" => Ok(AggFunc::Count),
        "avg" => Ok(AggFunc::Avg),
        other => Err(ParseError::Unsupported(format!(
            "unknown aggregate function: {other}"
        ))),
    }
}

fn lower_projection(items: Vec<s::SelectItem>) -> Result<Projection, ParseError> {
    let exprs = items
        .into_iter()
        .map(|item| match item {
            s::SelectItem::UnnamedExpr(s::Expr::Identifier(id)) => Ok(SelectExpr::Col(id.value)),
            s::SelectItem::UnnamedExpr(s::Expr::Function(f)) => {
                let func_name = f.name.to_string();
                let func = lower_agg_func(&func_name)?;
                let col = match f.args {
                    s::FunctionArguments::List(list) => match list.args.into_iter().next() {
                        Some(s::FunctionArg::Unnamed(s::FunctionArgExpr::Wildcard)) => "*".into(),
                        Some(s::FunctionArg::Unnamed(s::FunctionArgExpr::Expr(
                            s::Expr::Identifier(id),
                        ))) => id.value,
                        other => {
                            return Err(ParseError::Unsupported(format!(
                                "unsupported function argument: {:?}",
                                other
                            )));
                        }
                    },
                    other => {
                        return Err(ParseError::Unsupported(format!(
                            "unsupported function args form: {:?}",
                            other
                        )));
                    }
                };
                Ok(SelectExpr::Agg { func, col })
            }
            other => Err(ParseError::Unsupported(format!(
                "unsupported projection item: {:?}",
                other
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Projection::Exprs(exprs))
}

fn lower_group_by(group_by: s::GroupByExpr) -> Result<Vec<String>, ParseError> {
    match group_by {
        // No GROUP BY clause — normal case.
        s::GroupByExpr::Expressions(exprs, _) if exprs.is_empty() => Ok(vec![]),

        s::GroupByExpr::Expressions(exprs, _) => exprs
            .into_iter()
            .map(|e| match e {
                s::Expr::Identifier(id) => Ok(id.value),
                other => Err(ParseError::Unsupported(format!(
                    "GROUP BY only supports column names, got: {:?}",
                    other
                ))),
            })
            .collect(),

        // GROUP BY ALL or other exotic forms — not supported.
        other => Err(ParseError::Unsupported(format!(
            "unsupported GROUP BY form: {:?}",
            other
        ))),
    }
}

fn lower_select(query: s::Query) -> Result<SelectStmt, ParseError> {
    let select = match *query.body {
        s::SetExpr::Select(s) => s,
        _ => {
            return Err(ParseError::Unsupported(
                "only plain SELECT supported".into(),
            ));
        }
    };
    let table = select
        .from
        .into_iter()
        .next()
        .ok_or_else(|| ParseError::Unsupported("SELECT requires a FROM clause".into()))
        .and_then(|t| match t.relation {
            s::TableFactor::Table { name, .. } => Ok(name.to_string()),
            _ => Err(ParseError::Unsupported(
                "only simple table references supported".into(),
            )),
        })?;
    let projection = if select.projection.len() == 1 {
        if let s::SelectItem::Wildcard(_) = &select.projection[0] {
            Projection::All
        } else {
            lower_projection(select.projection)?
        }
    } else {
        lower_projection(select.projection)?
    };

    let where_clause = select.selection.map(lower_predicate).transpose()?;
    let group_by = lower_group_by(select.group_by)?;
    // Validate: mixing Col and Agg is only legal when GROUP BY is present,
    // and every bare column must appear in the GROUP BY list.
    if let Projection::Exprs(ref exprs) = projection {
        let has_col = exprs.iter().any(|e| matches!(e, SelectExpr::Col(_)));
        let has_agg = exprs.iter().any(|e| matches!(e, SelectExpr::Agg { .. }));

        if has_col && has_agg {
            if group_by.is_empty() {
                return Err(ParseError::Unsupported(
                    "mixing columns and aggregates requires a GROUP BY clause".into(),
                ));
            }
            // Every bare column must be in the GROUP BY list.
            for expr in exprs {
                if let SelectExpr::Col(name) = expr {
                    if !group_by.contains(name) {
                        return Err(ParseError::Unsupported(format!(
                            "column '{name}' must appear in GROUP BY or be an aggregate"
                        )));
                    }
                }
            }
        }
    }

    Ok(SelectStmt {
        table,
        projection,
        where_clause,
        group_by,
    })
}

#[cfg(test)]
mod tests {
    use crate::parser::ast::{AggFunc, CmpOp, Literal, Predicate, Projection, SelectExpr};
    use crate::parser::{Statement, parse};

    #[test]
    fn test_where_lowering() {
        let sql = "SELECT * FROM events WHERE ts > 1000 AND uid = 42";
        let stmt = parse(sql).unwrap();
        let select = match stmt {
            Statement::Select(s) => s,
            _ => panic!("expected SELECT"),
        };
        assert_eq!(
            select.where_clause,
            Some(Predicate::And(
                Box::new(Predicate::Cmp {
                    col: "ts".into(),
                    op: CmpOp::Gt,
                    value: Literal::Int(1000)
                }),
                Box::new(Predicate::Cmp {
                    col: "uid".into(),
                    op: CmpOp::Eq,
                    value: Literal::Int(42)
                }),
            ))
        );
    }

    #[test]
    fn test_agg_lowering() {
        let sql = "SELECT sum(ts), count(*), max(uid) FROM events";
        let stmt = parse(sql).unwrap();
        let select = match stmt {
            Statement::Select(s) => s,
            _ => panic!("expected SELECT"),
        };
        let exprs = match select.projection {
            Projection::Exprs(e) => e,
            _ => panic!("expected Exprs"),
        };
        assert_eq!(
            exprs[0],
            SelectExpr::Agg {
                func: AggFunc::Sum,
                col: "ts".into()
            }
        );
        assert_eq!(
            exprs[1],
            SelectExpr::Agg {
                func: AggFunc::Count,
                col: "*".into()
            }
        );
        assert_eq!(
            exprs[2],
            SelectExpr::Agg {
                func: AggFunc::Max,
                col: "uid".into()
            }
        );
    }
}
