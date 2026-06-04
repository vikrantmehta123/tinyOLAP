//! Type Coercion Rule
//!
//! The logical plan uses untyped literals (e.g. `LiteralValue::Int(30)`).
//! This rule inserts a `Cast` node around each literal in a
//! comparison expr so downstream lowering knows the exact target type.
//! Otherwise we get type mismatch errors when we compare i32 vs i16.

use crate::{
    catalog::schema::TableSchema,
    logical_plan::{
        logical_operators::{BinaryOp, LogicalExpr, LogicalPlan},
        optimizer::{OptimizerRule, rewrite},
    },
};

pub struct TypeCoercion<'a> {
    pub schema: &'a TableSchema,
}

impl<'a> OptimizerRule for TypeCoercion<'a> {
    fn name(&self) -> &str {
        "type_coercion"
    }
    fn apply(&self, plan: LogicalPlan) -> LogicalPlan {
        // Traverse the LogicalPlan tree. Apply TypeCoercion only when we see a Filter
        rewrite(plan, &|node| match node {
            LogicalPlan::Filter { predicate, input } => LogicalPlan::Filter {
                // Recurse into the predicate expression, since predicate is also supposed to a tree
                predicate: coerce_expr(predicate, self.schema),
                input,
            },
            other => other,
        })
    }
}

/// Recursively traverse the ExpressionTree to identify BinaryOps.
/// Only on ops like Gt, Lt, Eq, Nq, GtEq, LtEq, can we apply coercion.
/// For logical comparisons like And, Not, Or- we cannot.
fn coerce_expr(expr: LogicalExpr, schema: &TableSchema) -> LogicalExpr {
    match expr {
        LogicalExpr::BinaryOp { left, op, right } => {
            let left = coerce_expr(*left, schema);
            let right = coerce_expr(*right, schema);

            if is_comparison_op(&op) {
                coerce_comparison(left, op, right, schema)
            } else {
                LogicalExpr::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                }
            }
        }

        LogicalExpr::Cast {
            expr,
            target_datatype,
        } => LogicalExpr::Cast {
            expr: Box::new(coerce_expr(*expr, schema)),
            target_datatype,
        },

        other => other,
    }
}

/// Insert the Cast operator where it is actually required
fn coerce_comparison(
    left: LogicalExpr,
    op: BinaryOp,
    right: LogicalExpr,
    schema: &TableSchema,
) -> LogicalExpr {
    match (left, right) {
        (LogicalExpr::Column(table, col), LogicalExpr::Literal(lit)) => {
            let target_type = schema
                .columns
                .iter()
                .find(|c| c.name == col)
                .expect("analyzer should have validated column existence")
                .data_type
                .clone();

            LogicalExpr::BinaryOp {
                left: Box::new(LogicalExpr::Column(table, col)),
                op,
                right: Box::new(LogicalExpr::Cast {
                    expr: Box::new(LogicalExpr::Literal(lit)),
                    target_datatype: target_type,
                }),
            }
        }

        (LogicalExpr::Literal(lit), LogicalExpr::Column(table, col)) => {
            let target_type = schema
                .columns
                .iter()
                .find(|c| c.name == col)
                .expect("analyzer should have validated column existence")
                .data_type
                .clone();

            LogicalExpr::BinaryOp {
                left: Box::new(LogicalExpr::Cast {
                    expr: Box::new(LogicalExpr::Literal(lit)),
                    target_datatype: target_type,
                }),
                op,
                right: Box::new(LogicalExpr::Column(table, col)),
            }
        }

        (left, right) => LogicalExpr::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        },
    }
}

fn is_comparison_op(op: &BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::LtEq
            | BinaryOp::Gt
            | BinaryOp::GtEq
    )
}
