//! Eliminates Always True Filters

use crate::logical_plan::{logical_operators::{LiteralValue, LogicalExpr, LogicalPlan}, optimizer::{OptimizerRule, rewrite}};

pub struct EliminateTrueFilter;

impl OptimizerRule for EliminateTrueFilter {
    fn name(&self) -> &str { "eliminate_true_filter" }

    fn apply(&self, plan: LogicalPlan) -> LogicalPlan {
        // Runs after ConstantFolding — catches filters that folded to true
        rewrite(plan, &|node| match node {
            LogicalPlan::Filter { predicate: LogicalExpr::Literal(LiteralValue::Bool(true)), input } => *input,
            other => other,
        })
    }
}