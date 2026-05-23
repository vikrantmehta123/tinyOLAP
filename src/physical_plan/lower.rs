use std::collections::BTreeSet;


use crate::logical_plan::logical_operators::{
    AggFunc as LogAggFunc, BinaryOp as LogBinaryOp, LiteralValue as LogLiteralValue,
    LogicalExpr, LogicalPlan,
};
use crate::physical_plan::physical_operators::{
    AggFunc as PhysAggFunc, AggSpec, BinaryOp as PhysBinaryOp, LiteralValue as PhysLiteralValue, PhysicalExpr, PhysicalPlan,
};

pub fn lower(plan: LogicalPlan) -> PhysicalPlan {
    let mut cols = BTreeSet::new();

    // Recursively collect the column names in the SQL query
    collect_columns(&plan, &mut cols);
    lower_plan(plan, &cols)
}

fn collect_columns(plan: &LogicalPlan, cols: &mut BTreeSet<String>) {
    match plan {
        LogicalPlan::Scan { .. } => {}
        LogicalPlan::Filter { predicate, input } => {
            collect_expr_columns(predicate, cols);
            collect_columns(input, cols);
        }
        LogicalPlan::Project { projections, input } => {
            for expr in projections {
                collect_expr_columns(expr, cols);
            }
            collect_columns(input, cols);
        }
        LogicalPlan::Aggregate {
            group_by,
            aggregates,
            input,
        } => {
            for expr in group_by.iter().chain(aggregates.iter()) {
                collect_expr_columns(expr, cols);
            }
            collect_columns(input, cols);
        }
        LogicalPlan::Limit { input, .. } => collect_columns(input, cols),
    }
}

fn collect_expr_columns(expr: &LogicalExpr, cols: &mut BTreeSet<String>) {
    match expr {
        LogicalExpr::Column(_, col) => {
            cols.insert(col.clone());
        }
        LogicalExpr::Literal(_) => {}
        LogicalExpr::BinaryOp { left, right, .. } => {
            collect_expr_columns(left, cols);
            collect_expr_columns(right, cols);
        }
        LogicalExpr::Aggregate { arg, .. } => collect_expr_columns(arg, cols),
    }
}

fn lower_plan(plan: LogicalPlan, cols: &BTreeSet<String>) -> PhysicalPlan {
    match plan {
        LogicalPlan::Scan { table } => PhysicalPlan::Scan {
            table,
            columns: cols.iter().cloned().collect(),
        },
        LogicalPlan::Filter { predicate, input } => PhysicalPlan::Filter {
            predicate: lower_expr(predicate),
            input: Box::new(lower_plan(*input, cols)),
        },
        LogicalPlan::Project { projections, input } => PhysicalPlan::Project {
            projections: projections.into_iter().map(lower_expr).collect(),
            input: Box::new(lower_plan(*input, cols)),
        },
        LogicalPlan::Aggregate { group_by, aggregates, input } =>  PhysicalPlan::Aggregate {
            group_by: group_by.into_iter().map(lower_expr).collect(),
            aggregates: aggregates.into_iter().map(lower_agg_spec).collect(),
            input: Box::new(lower_plan(*input, cols)),   
        }, 
        LogicalPlan::Limit { limit, input } => PhysicalPlan::Limit { 
            limit, 
            input: Box::new(lower_plan(*input, cols)),
        }
    }
}

fn lower_expr(expr: LogicalExpr) -> PhysicalExpr {
    match expr {
        LogicalExpr::Column(_,  col) => PhysicalExpr::Column(col), 
        LogicalExpr::Literal(lit) => PhysicalExpr::Literal(lower_literal(lit)), 
        LogicalExpr::BinaryOp { left, op, right } => PhysicalExpr::BinaryOp { 
            left: Box::new(lower_expr(*left)),
            op: lower_binary_op(op), 
            right: Box::new(lower_expr(*right)),
        }, 
        LogicalExpr::Aggregate { func, arg } => {
            PhysicalExpr::Column(format!("{}({})", agg_func_name(&func), expr_display(&arg)))
        }
    }
}

fn lower_agg_spec(expr: LogicalExpr) -> AggSpec {
    match expr {
        LogicalExpr::Aggregate { func, arg } => {
            let output_name = format!("{}({})", agg_func_name(&func), expr_display(&arg));
            AggSpec {
                func: lower_agg_func(func),
                arg: lower_expr(*arg),
                output_name,
            }
        }
        _ => panic!("lower_agg_spec: expected Aggregate expression"),
    }
}

fn lower_agg_func(func: LogAggFunc) -> crate::physical_plan::physical_operators::AggFunc {
    match func {
        LogAggFunc::Count => PhysAggFunc::Count,
        LogAggFunc::Sum   => PhysAggFunc::Sum,
        LogAggFunc::Avg   => PhysAggFunc::Avg,
        LogAggFunc::Min   => PhysAggFunc::Min,
        LogAggFunc::Max   => PhysAggFunc::Max,
    }
}

fn lower_binary_op(op: LogBinaryOp) -> crate::physical_plan::physical_operators::BinaryOp {
    match op {
        LogBinaryOp::Eq    => PhysBinaryOp::Eq,
        LogBinaryOp::NotEq => PhysBinaryOp::NotEq,
        LogBinaryOp::Lt    => PhysBinaryOp::Lt,
        LogBinaryOp::LtEq  => PhysBinaryOp::LtEq,
        LogBinaryOp::Gt    => PhysBinaryOp::Gt,
        LogBinaryOp::GtEq  => PhysBinaryOp::GtEq,
        LogBinaryOp::And   => PhysBinaryOp::And,
        LogBinaryOp::Or    => PhysBinaryOp::Or,
    }
}

fn lower_literal(lit: LogLiteralValue) -> crate::physical_plan::physical_operators::LiteralValue {
    match lit {
        LogLiteralValue::Int(n)   => PhysLiteralValue::Int(n),
        LogLiteralValue::Float(f) => PhysLiteralValue::Float(f),
        LogLiteralValue::Str(s)   => PhysLiteralValue::Str(s),
        LogLiteralValue::Bool(b)  => PhysLiteralValue::Bool(b),
        LogLiteralValue::Null     => PhysLiteralValue::Null,
    }
}

fn agg_func_name(func: &LogAggFunc) -> &'static str {
    match func {
        LogAggFunc::Count => "count",
        LogAggFunc::Sum   => "sum",
        LogAggFunc::Avg   => "avg",
        LogAggFunc::Min   => "min",
        LogAggFunc::Max   => "max",
    }
}

fn expr_display(expr: &LogicalExpr) -> String {
    match expr {
        LogicalExpr::Column(_, col)                  => col.clone(),
        LogicalExpr::Literal(LogLiteralValue::Int(n))   => n.to_string(),
        LogicalExpr::Literal(LogLiteralValue::Float(f)) => f.to_string(),
        LogicalExpr::Literal(LogLiteralValue::Str(s))   => s.clone(),
        LogicalExpr::Literal(LogLiteralValue::Bool(b))  => b.to_string(),
        LogicalExpr::Literal(LogLiteralValue::Null)      => "null".to_string(),
        LogicalExpr::BinaryOp { .. }                 => "<expr>".to_string(),
        LogicalExpr::Aggregate { func, arg }         => {
            format!("{}({})", agg_func_name(func), expr_display(arg))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::lower;
    use crate::logical_plan::logical_operators::{
        AggFunc as LogAggFunc, BinaryOp as LogBinaryOp, LiteralValue as LogLiteralValue,
        LogicalExpr, LogicalPlan,
    };
    use crate::physical_plan::physical_operators::{AggFunc, BinaryOp, LiteralValue, PhysicalExpr, PhysicalPlan};

    fn col(table: &str, name: &str) -> LogicalExpr {
        LogicalExpr::Column(table.to_string(), name.to_string())
    }

    // SELECT name FROM users
    // Verifies: Scan only loads referenced columns
    #[test]
    fn test_column_pruning() {
        let plan = LogicalPlan::Project {
            projections: vec![col("users", "name")],
            input: Box::new(LogicalPlan::Scan { table: "users".to_string() }),
        };

        let physical = lower(plan);

        match physical {
            PhysicalPlan::Project { input, .. } => match *input {
                PhysicalPlan::Scan { table, columns } => {
                    assert_eq!(table, "users");
                    assert_eq!(columns, vec!["name"]);
                }
                _ => panic!("expected Scan"),
            },
            _ => panic!("expected Project"),
        }
    }

    // SELECT name FROM users WHERE age > 30
    // Verifies: Filter predicate lowered correctly, both name + age in Scan
    #[test]
    fn test_filter_lowering() {
        let plan = LogicalPlan::Project {
            projections: vec![col("users", "name")],
            input: Box::new(LogicalPlan::Filter {
                predicate: LogicalExpr::BinaryOp {
                    left: Box::new(col("users", "age")),
                    op: LogBinaryOp::Gt,
                    right: Box::new(LogicalExpr::Literal(LogLiteralValue::Int(30))),
                },
                input: Box::new(LogicalPlan::Scan { table: "users".to_string() }),
            }),
        };

        let physical = lower(plan);

        match physical {
            PhysicalPlan::Project { input, .. } => match *input {
                PhysicalPlan::Filter { predicate, input } => {
                    match predicate {
                        PhysicalExpr::BinaryOp { left, op, right } => {
                            match (*left, op, *right) {
                                (
                                    PhysicalExpr::Column(col),
                                    BinaryOp::Gt,
                                    PhysicalExpr::Literal(LiteralValue::Int(30)),
                                ) => assert_eq!(col, "age"),
                                _ => panic!("unexpected predicate shape"),
                            }
                        }
                        _ => panic!("expected BinaryOp predicate"),
                    }
                    match *input {
                        PhysicalPlan::Scan { mut columns, .. } => {
                            columns.sort();
                            assert_eq!(columns, vec!["age", "name"]);
                        }
                        _ => panic!("expected Scan"),
                    }
                }
                _ => panic!("expected Filter"),
            },
            _ => panic!("expected Project"),
        }
    }

    // SELECT name, SUM(age) FROM users GROUP BY name
    // Verifies: aggregate → AggSpec with correct output_name,
    //           project sees Column("sum(age)") not an aggregate expr
    #[test]
    fn test_aggregate_lowering() {
        let agg_expr = || LogicalExpr::Aggregate {
            func: LogAggFunc::Sum,
            arg: Box::new(col("users", "age")),
        };

        let plan = LogicalPlan::Project {
            projections: vec![col("users", "name"), agg_expr()],
            input: Box::new(LogicalPlan::Aggregate {
                group_by: vec![col("users", "name")],
                aggregates: vec![agg_expr()],
                input: Box::new(LogicalPlan::Scan { table: "users".to_string() }),
            }),
        };

        let physical = lower(plan);

        match physical {
            PhysicalPlan::Project { projections, input } => {
                assert_eq!(projections.len(), 2);
                match &projections[0] {
                    PhysicalExpr::Column(c) => assert_eq!(c, "name"),
                    _ => panic!("expected Column for name"),
                }
                // Aggregate in project must become a Column reference, not an aggregate expr
                match &projections[1] {
                    PhysicalExpr::Column(c) => assert_eq!(c, "sum(age)"),
                    _ => panic!("expected Column(\"sum(age)\") in projection"),
                }

                match *input {
                    PhysicalPlan::Aggregate { group_by, aggregates, input } => {
                        assert_eq!(group_by.len(), 1);
                        match &group_by[0] {
                            PhysicalExpr::Column(c) => assert_eq!(c, "name"),
                            _ => panic!("expected Column in group_by"),
                        }

                        assert_eq!(aggregates.len(), 1);
                        assert_eq!(aggregates[0].output_name, "sum(age)");
                        match aggregates[0].func {
                            AggFunc::Sum => {}
                            _ => panic!("expected Sum"),
                        }
                        match &aggregates[0].arg {
                            PhysicalExpr::Column(c) => assert_eq!(c, "age"),
                            _ => panic!("expected Column(\"age\") as agg arg"),
                        }

                        match *input {
                            PhysicalPlan::Scan { mut columns, .. } => {
                                columns.sort();
                                assert_eq!(columns, vec!["age", "name"]);
                            }
                            _ => panic!("expected Scan"),
                        }
                    }
                    _ => panic!("expected Aggregate"),
                }
            }
            _ => panic!("expected Project"),
        }
    }

    // SELECT name FROM users LIMIT 5
    // Verifies: Limit wraps the rest of the tree correctly
    #[test]
    fn test_limit_lowering() {
        let plan = LogicalPlan::Limit {
            limit: 5,
            input: Box::new(LogicalPlan::Project {
                projections: vec![col("users", "name")],
                input: Box::new(LogicalPlan::Scan { table: "users".to_string() }),
            }),
        };

        let physical = lower(plan);

        match physical {
            PhysicalPlan::Limit { limit, input } => {
                assert_eq!(limit, 5);
                match *input {
                    PhysicalPlan::Project { .. } => {}
                    _ => panic!("expected Project under Limit"),
                }
            }
            _ => panic!("expected Limit"),
        }
    }
}
