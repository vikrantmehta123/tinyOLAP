//! Constant Folding Optimization
//! 
//! Applies Constant Folding Optimization on Logical Plan tree


use crate::logical_plan::{logical_operators::{BinaryOp, LiteralValue, LogicalExpr, LogicalPlan}, optimizer::{OptimizerRule, rewrite}};

pub struct ConstantFolding;

// Evaluates BinaryOp expressions where both sides are literals.
fn fold_expr(expr: LogicalExpr) -> LogicalExpr {
    match expr {
        LogicalExpr::BinaryOp { left, op, right } => {
            let left = fold_expr(*left);
            let right = fold_expr(*right);
            match (&left, &op, &right) {
                // Int comparisons → Bool
                (LogicalExpr::Literal(LiteralValue::Int(a)), _, LogicalExpr::Literal(LiteralValue::Int(b))) => {
                    let result = match op {
                        BinaryOp::Eq    => a == b,
                        BinaryOp::NotEq => a != b,
                        BinaryOp::Lt    => a < b,
                        BinaryOp::LtEq  => a <= b,
                        BinaryOp::Gt    => a > b,
                        BinaryOp::GtEq  => a >= b,
                        _ => return LogicalExpr::BinaryOp { left: Box::new(left), op, right: Box::new(right) },
                    };
                    LogicalExpr::Literal(LiteralValue::Bool(result))
                }
                // Bool AND/OR short-circuit folding
                (LogicalExpr::Literal(LiteralValue::Bool(true)),  BinaryOp::And, _) => right, // true and x = x
                (LogicalExpr::Literal(LiteralValue::Bool(false)), BinaryOp::And, _) => LogicalExpr::Literal(LiteralValue::Bool(false)), // false AND anything = false
                (LogicalExpr::Literal(LiteralValue::Bool(true)),  BinaryOp::Or,  _) => LogicalExpr::Literal(LiteralValue::Bool(true)), // true OR anything = true
                (LogicalExpr::Literal(LiteralValue::Bool(false)), BinaryOp::Or,  _) => right, // false OR x = x
                _ => LogicalExpr::BinaryOp { left: Box::new(left), op, right: Box::new(right) },
            }
        }
        other => other,
    }
}

impl OptimizerRule for ConstantFolding {
    fn name(&self) -> &str { "constant_folding" }

    fn apply(&self, plan: LogicalPlan) -> LogicalPlan {
        rewrite(plan, &|node| match node {
            LogicalPlan::Filter { predicate, input } => LogicalPlan::Filter {
                predicate: fold_expr(predicate),
                input,
            },
            LogicalPlan::Project { projections, input } => LogicalPlan::Project {
                projections: projections.into_iter().map(fold_expr).collect(),
                input,
            },
            other => other,
        })
    }
}