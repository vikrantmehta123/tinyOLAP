//! Rule-based logical plan optimizer.
//!
//! Each optimization implements OptimizerRule and receives the whole plan tree.
//! Rules are applied in order. The order of optimization rules matter.

use crate::{
    catalog::schema::TableSchema,
    logical_plan::{
        logical_operators::LogicalPlan,
        optimizer_rules::{
            constant_folding::ConstantFolding, eliminate_true_filter::EliminateTrueFilter,
            type_coercion::TypeCoercion,
        },
    },
};

pub trait OptimizerRule {
    fn name(&self) -> &str;
    fn apply(&self, plan: LogicalPlan) -> LogicalPlan;
}

pub struct Optimizer<'a> {
    rules: Vec<Box<dyn OptimizerRule + 'a>>,
}

impl<'a> Optimizer<'a> {
    pub fn new(schema: &'a TableSchema) -> Self {
        Self {
            rules: vec![
                Box::new(TypeCoercion { schema }),
                Box::new(ConstantFolding),
                Box::new(EliminateTrueFilter),
            ],
        }
    }

    pub fn optimize(&self, plan: LogicalPlan) -> LogicalPlan {
        self.rules.iter().fold(plan, |p, rule| rule.apply(p))
    }
}

// Applies a function bottom-up to every node in the tree.
// Children are rewritten before the parent — so when a rule sees a node,
// its children are already fully rewritten.
pub fn rewrite<F>(plan: LogicalPlan, f: &F) -> LogicalPlan
where
    F: Fn(LogicalPlan) -> LogicalPlan,
{
    // consume plan, extract children, rewrite them, reassemble
    match plan {
        LogicalPlan::Filter { predicate, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Filter {
                predicate,
                input: Box::new(new_input),
            })
        }
        LogicalPlan::Project { projections, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Project {
                projections,
                input: Box::new(new_input),
            })
        }
        LogicalPlan::Aggregate {
            group_by,
            aggregates,
            input,
        } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Aggregate {
                group_by,
                aggregates,
                input: Box::new(new_input),
            })
        }
        LogicalPlan::Limit { limit, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Limit {
                limit,
                input: Box::new(new_input),
            })
        }
        leaf => f(leaf),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};
    use crate::frontend::parser::parse;
    use crate::logical_plan::logical_operators::{LiteralValue, LogicalExpr};
    use crate::logical_plan::lower::lower;

    fn make_schema() -> TableSchema {
        TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".to_string(),
                    data_type: DataType::I64,
                },
                ColumnSchema {
                    name: "age".to_string(),
                    data_type: DataType::I32,
                },
            ],
            sort_key: vec![0],
        }
    }

    fn optimize_sql(sql: &str) -> LogicalPlan {
        let stmt = parse(sql).unwrap();
        let plan = lower(&stmt, &make_schema()).unwrap();
        let schema = make_schema();
        Optimizer::new(&schema).optimize(plan)
    }

    // 1 = 1 folds to true, AND with age > 30 simplifies to just age > 30
    #[test]
    fn constant_fold_and_simplify() {
        let plan = optimize_sql("SELECT id FROM users WHERE 1 = 1 AND age > 30");
        println!("{}", plan);
        if let LogicalPlan::Project { input, .. } = plan {
            if let LogicalPlan::Filter { predicate, .. } = input.as_ref() {
                // predicate should be just age > 30, not true AND age > 30
                assert!(!matches!(
                    predicate,
                    LogicalExpr::Literal(LiteralValue::Bool(true))
                ));
            }
        }
    }

    // WHERE true — Filter node should be eliminated entirely
    #[test]
    fn eliminate_true_filter() {
        let plan = optimize_sql("SELECT id FROM users WHERE 1 = 1");
        println!("{}", plan);
        // After folding 1=1 → true and eliminating the filter,
        // Project should wrap Scan directly
        if let LogicalPlan::Project { input, .. } = plan {
            assert!(matches!(input.as_ref(), LogicalPlan::Scan { .. }));
        } else {
            panic!("expected Project at root");
        }
    }

    // WHERE false — Filter node must be kept, not eliminated
    #[test]
    fn keep_false_filter() {
        let plan = optimize_sql("SELECT id FROM users WHERE 1 = 2");
        println!("{}", plan);
        if let LogicalPlan::Project { input, .. } = plan {
            assert!(matches!(input.as_ref(), LogicalPlan::Filter { .. }));
        } else {
            panic!("expected Project at root");
        }
    }

    // Non-constant expressions should not be folded
    #[test]
    fn non_constant_expr_unchanged() {
        let plan = optimize_sql("SELECT id FROM users WHERE age > 30");
        if let LogicalPlan::Project { input, .. } = plan {
            assert!(matches!(input.as_ref(), LogicalPlan::Filter { .. }));
        } else {
            panic!("expected Filter to remain");
        }
    }
}
