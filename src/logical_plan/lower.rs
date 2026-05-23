//! Lowers a validated sqlparser Statement into a LogicalPlan.
//! Name resolution happens here — bare column names are fully qualified.

use crate::catalog::schema::TableSchema;
use crate::logical_plan::logical_operators::{
    AggFunc, BinaryOp, LiteralValue, LogicalExpr, LogicalPlan,
};
use sqlparser::ast::{
    Expr, GroupByExpr, SelectItem, SetExpr, Statement,
};

pub fn lower(stmt: &Statement, schema: &TableSchema) -> Result<LogicalPlan, String> {
    match stmt {
        Statement::Query(query) => lower_query(query, schema),
        _ => Err("only SELECT is supported in the lowerer".to_string()),
    }
}

fn lower_query(query: &sqlparser::ast::Query, schema: &TableSchema) -> Result<LogicalPlan, String> {
    let select = match query.body.as_ref() {
        SetExpr::Select(s) => s,
        _ => return Err("Only SELECT is supported".to_string()),
    };

    // Build the plan bottom-up.
    let mut plan = LogicalPlan::Scan {
        table: schema.name.clone(),
    };

    if let Some(predicate) = &select.selection {
        let predicate = lower_expr(predicate, schema)?;
        plan = LogicalPlan::Filter {
            predicate,
            input: Box::new(plan),
        };
    }

    let group_by = lower_group_by(&select.group_by, schema)?;
    let aggregates = collect_aggregates(&select.projection, schema)?;

    if !group_by.is_empty() || !aggregates.is_empty() {
        plan = LogicalPlan::Aggregate {
            group_by,
            aggregates,
            input: Box::new(plan),
        };
    }

    let projections = lower_projection(&select.projection, schema)?;
    plan = LogicalPlan::Project {
        projections: projections,
        input: Box::new(plan),
    };

    if let Some(limit_expr) = &query.limit_clause {
        let limit = lower_limit(limit_expr)?;
        plan = LogicalPlan::Limit {
            limit,
            input: Box::new(plan),
        };
    }

    Ok(plan)
}

// Converts a sqlparser Expr to a LogicalExpr.
// Name resolution happens here — bare identifiers are qualified with the table name.
fn lower_expr(expr: &Expr, schema: &TableSchema) -> Result<LogicalExpr, String> {
    match expr {
        Expr::Identifier(ident) => {
            // Resolve a bare column name and qualify it with its table name
            let col = schema
                .columns
                .iter()
                .find(|c| c.name == ident.value)
                .ok_or_else(|| format!("Unknown column: {}", ident.value))?;

            Ok(LogicalExpr::Column(schema.name.clone(), col.name.clone()))
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => Ok(LogicalExpr::Column(
            parts[0].value.clone(),
            parts[1].value.clone(),
        )),
        Expr::Value(val) => Ok(LogicalExpr::Literal(lower_literal(val)?)),
        Expr::BinaryOp { left, op, right } => {
            let left = lower_expr(left, schema)?;
            let right = lower_expr(right, schema)?;
            let op = lower_binary_op(op)?;

            Ok(LogicalExpr::BinaryOp {
                left: Box::new(left),
                op: op,
                right: Box::new(right),
            })
        }, 
        Expr::Function(f) => lower_agg_expr(f, schema),
        _ => Err(format!("Unsupported expression: {:?}", expr)),
    }
}

// sqlparser represents all numeric literals as strings — we try i64 first,
// then fall back to f64, since SQL users don't annotate literal types.
fn lower_literal(val: &sqlparser::ast::ValueWithSpan) -> Result<LiteralValue, String> {
    use sqlparser::ast::Value;
    match &val.value {
        Value::Number(n, _) => {
            if let Ok(i) = n.parse::<i64>() {
                Ok(LiteralValue::Int(i))
            } else if let Ok(f) = n.parse::<f64>() {
                Ok(LiteralValue::Float(f))
            } else {
                Err(format!("cannot parse number: {}", n))
            }
        }
        Value::SingleQuotedString(s) => Ok(LiteralValue::Str(s.clone())),
        Value::Boolean(b) => Ok(LiteralValue::Bool(*b)),
        Value::Null => Ok(LiteralValue::Null),
        _ => Err(format!("unsupported literal: {:?}", val.value)),
    }
}

fn lower_binary_op(op: &sqlparser::ast::BinaryOperator) -> Result<BinaryOp, String> {
    use sqlparser::ast::BinaryOperator::*;
    match op {
        Eq => Ok(BinaryOp::Eq),
        NotEq => Ok(BinaryOp::NotEq),
        Lt => Ok(BinaryOp::Lt),
        LtEq => Ok(BinaryOp::LtEq),
        Gt => Ok(BinaryOp::Gt),
        GtEq => Ok(BinaryOp::GtEq),
        And => Ok(BinaryOp::And),
        Or => Ok(BinaryOp::Or),
        _ => Err(format!("unsupported operator: {:?}", op)),
    }
}

// SELECT * expands to one Column expression per schema column, in order.
fn lower_projection(
    projection: &[SelectItem],
    schema: &TableSchema,
) -> Result<Vec<LogicalExpr>, String> {
    let mut exprs = vec![];
    for item in projection {
        match item {
            // SELECT * expands to all columns, fully qualified
            SelectItem::Wildcard(_) => {
                for col in &schema.columns {
                    exprs.push(LogicalExpr::Column(schema.name.clone(), col.name.clone()));
                }
            }
            SelectItem::UnnamedExpr(expr) => exprs.push(lower_expr(expr, schema)?),
            SelectItem::ExprWithAlias { expr, .. } => exprs.push(lower_expr(expr, schema)?),
            _ => return Err("unsupported SELECT item".to_string()),
        }
    }
    Ok(exprs)
}

fn lower_group_by(
    group_by: &GroupByExpr,
    schema: &TableSchema,
) -> Result<Vec<LogicalExpr>, String> {
    match group_by {
        GroupByExpr::Expressions(exprs, _) => exprs.iter().map(|e| lower_expr(e, schema)).collect(),
        _ => Err("Unsupported GROUP BY form".to_string()),
    }
}

fn lower_limit(clause: &sqlparser::ast::LimitClause) -> Result<u64, String> {
    use sqlparser::ast::LimitClause;
    match clause {
        LimitClause::LimitOffset { limit: Some(Expr::Value(val)), .. } => {
            if let sqlparser::ast::Value::Number(n, _) = &val.value {
                n.parse::<u64>().map_err(|_| format!("invalid LIMIT value: {}", n))
            } else {
                Err("LIMIT must be a number".to_string())
            }
        }
        _ => Err("unsupported LIMIT clause".to_string()),
    }
}

// Scans the SELECT list and collects only aggregate function expressions.
// These become the aggregates on the Aggregate node, separate from group_by columns.
fn collect_aggregates(
    projection: &[SelectItem],
    schema: &TableSchema,
) -> Result<Vec<LogicalExpr>, String> {
    let mut aggregates = vec![];
    for item in projection {
        let expr = match item {
            SelectItem::UnnamedExpr(e) => e,
            SelectItem::ExprWithAlias { expr, .. } => expr,
            _ => continue,
        };
        if let Expr::Function(_) = expr {
            aggregates.push(lower_expr(expr, schema)?);
        }
    }
    Ok(aggregates)
}

fn lower_agg_expr(
    f: &sqlparser::ast::Function,
    schema: &TableSchema,
) -> Result<LogicalExpr, String> {
    use sqlparser::ast::{FunctionArg, FunctionArgExpr, FunctionArguments};

    let func = match f.name.to_string().to_uppercase().as_str() {
        "COUNT" => AggFunc::Count,
        "SUM" => AggFunc::Sum,
        "AVG" => AggFunc::Avg,
        "MIN" => AggFunc::Min,
        "MAX" => AggFunc::Max,
        other => return Err(format!("unsupported aggregate function: {}", other)),
    };

    let arg = match &f.args {
        FunctionArguments::List(arg_list) => match arg_list.args.first() {
            Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(e))) => lower_expr(e, schema)?,
            _ => return Err("unsupported aggregate argument".to_string()),
        },
        _ => return Err("unsupported aggregate argument form".to_string()),
    };

    Ok(LogicalExpr::Aggregate {
        func,
        arg: Box::new(arg),
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::{ColumnSchema, TableSchema, DataType};
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

    fn lower_sql(sql: &str) -> Result<LogicalPlan, String> {
        let stmt = parse(sql).unwrap();
        lower(&stmt, &make_schema())
    }

    #[test]
    fn print_simple_select() {
        let plan = lower_sql("SELECT name, COUNT(id) FROM users GROUP BY name").unwrap();
        println!("{}", plan);
    }

    #[test]
    fn print_limit() {
        let plan = lower_sql("SELECT * FROM users LIMIT 10").unwrap();
        println!("{}", plan);
    }

    // Does a simple SELECT produce a Project at the root?
    #[test]
    fn simple_select_lowers_correctly() {
        let plan = lower_sql("SELECT id FROM users").unwrap();
        assert!(matches!(plan, LogicalPlan::Project { .. }));
    }

    // Is plan a Project whose child is a Filter?
    #[test]
    fn where_clause_produces_filter() {
        let plan = lower_sql("SELECT id FROM users WHERE age > 30").unwrap();
        assert!(matches!(plan, LogicalPlan::Project { 
            input, .. 
        } if matches!(input.as_ref(), LogicalPlan::Filter { .. })));
    }

    // Is Limit the root node with the correct value?
    #[test]
    fn limit_is_root() {
        let plan = lower_sql("SELECT id FROM users LIMIT 5").unwrap();
        assert!(matches!(plan, LogicalPlan::Limit { limit: 5, .. }));
    }

    // Does SELECT * expand to one Column expr per schema column?
    #[test]
    fn select_star_expands_to_all_columns() {
        let plan = lower_sql("SELECT * FROM users").unwrap();
        if let LogicalPlan::Project { projections, .. } = plan {
            assert_eq!(projections.len(), 3);
        } else {
            panic!("expected Project node");
        }
    }

}