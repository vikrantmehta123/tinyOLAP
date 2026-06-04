//! Eliminates Always True Filters
//!
//! Removes `Filter` nodes whose predicate is the literal `true`. Such filters pass
//! every row and are pure overhead. The typical source is a WHERE clause that
//! ConstantFolding already reduced to `true` (e.g. `WHERE 1 = 1`).
//! Thus, this rule should be applied after constant folding.
//!
//! Does NOT remove `Filter { predicate: false }`

use crate::logical_plan::{
    logical_operators::{LiteralValue, LogicalExpr, LogicalPlan},
    optimizer::{OptimizerRule, rewrite},
};

pub struct EliminateTrueFilter;

impl OptimizerRule for EliminateTrueFilter {
    fn name(&self) -> &str {
        "eliminate_true_filter"
    }

    fn apply(&self, plan: LogicalPlan) -> LogicalPlan {
        // Runs after ConstantFolding — catches filters that folded to true
        rewrite(plan, &|node| match node {
            LogicalPlan::Filter {
                predicate: LogicalExpr::Literal(LiteralValue::Bool(true)),
                input,
            } => *input,
            other => other,
        })
    }
}
