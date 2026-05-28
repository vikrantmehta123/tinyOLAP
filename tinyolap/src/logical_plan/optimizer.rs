//! Rule-based logical plan optimizer.
//! Each optimization implements OptimizerRule and receives the whole plan tree.
//! Rules are applied in order — the output of one becomes the input of the next.

use crate::{catalog::schema::TableSchema, logical_plan::logical_operators::{BinaryOp, LiteralValue, LogicalExpr, LogicalPlan}};

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
fn rewrite<F>(plan: LogicalPlan, f: &F) -> LogicalPlan
where
    F: Fn(LogicalPlan) -> LogicalPlan,
{
    // consume plan, extract children, rewrite them, reassemble
    match plan {
        LogicalPlan::Filter { predicate, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Filter { predicate, input: Box::new(new_input) })
        }
        LogicalPlan::Project { projections, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Project { projections, input: Box::new(new_input) })
        }
        LogicalPlan::Aggregate { group_by, aggregates, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Aggregate { group_by, aggregates, input: Box::new(new_input) })
        }
        LogicalPlan::Limit { limit, input } => {
            let new_input = rewrite(*input, f);
            f(LogicalPlan::Limit { limit, input: Box::new(new_input) })
        }
        leaf => f(leaf),
    }
}


struct ConstantFolding;

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

struct EliminateTrueFilter;

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

struct TypeCoercion<'a> {
    schema: &'a TableSchema,
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

/// Recursively traverse the ExpressionTree to identify BinaryOps. 
/// Only on ops like Gt, Lt, Eq, Nq, GtEq, LtEq, can we apply coercion
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

        LogicalExpr::Cast { expr, target_datatype } => LogicalExpr::Cast {
            expr: Box::new(coerce_expr(*expr, schema)),
            target_datatype,
        },

        other => other,
    }
}


impl<'a> OptimizerRule for TypeCoercion<'a> {
    fn name(&self) -> &str { "type_coercion" }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};
    use crate::frontend::parser::parse;
    use crate::logical_plan::lower::lower;

    fn make_schema() -> TableSchema {
        TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema { name: "id".to_string(),  data_type: DataType::I64 },
                ColumnSchema { name: "age".to_string(), data_type: DataType::I32 },
            ],
            sort_key: vec![0]
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
                assert!(!matches!(predicate, LogicalExpr::Literal(LiteralValue::Bool(true))));
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