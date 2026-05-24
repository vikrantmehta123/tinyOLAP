use crate::physical_plan::physical_operators::{LiteralValue, PhysicalExpr, PhysicalPlan};

pub trait OptimizerRule {
    fn name(&self) -> &str;
    fn apply(&self, plan: PhysicalPlan) -> PhysicalPlan;
}

pub struct Optimizer {
    rules: Vec<Box<dyn OptimizerRule>>,
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(EliminateTrueFilter),
                Box::new(PredicatePushdown),
            ],
        }
    }

    pub fn optimize(&self, plan: PhysicalPlan) -> PhysicalPlan {
        self.rules.iter().fold(plan, |p, rule| rule.apply(p))
    }
}

fn rewrite<F>(plan: PhysicalPlan, f: &F) -> PhysicalPlan
where
    F: Fn(PhysicalPlan) -> PhysicalPlan,
{
    match plan {
        PhysicalPlan::Filter { predicate, input } => {
            let new_input = rewrite(*input, f);
            f(PhysicalPlan::Filter { predicate, input: Box::new(new_input) })
        }
        PhysicalPlan::Project { projections, input } => {
            let new_input = rewrite(*input, f);
            f(PhysicalPlan::Project { projections, input: Box::new(new_input) })
        }
        PhysicalPlan::Aggregate { group_by, aggregates, input } => {
            let new_input = rewrite(*input, f);
            f(PhysicalPlan::Aggregate { group_by, aggregates, input: Box::new(new_input) })
        }
        PhysicalPlan::Limit { limit, input } => {
            let new_input = rewrite(*input, f);
            f(PhysicalPlan::Limit { limit, input: Box::new(new_input) })
        }
        leaf => f(leaf),
    }
}

struct PredicatePushdown;


// Choose which scan operator to use based on predicate
impl OptimizerRule for PredicatePushdown {
    fn name(&self) -> &str { "predicate_pushdown" }

    fn apply(&self, plan: PhysicalPlan) -> PhysicalPlan {
        rewrite(plan, &|node| match node {
            PhysicalPlan::Filter {
                predicate,
                input,
            } if matches!(*input, PhysicalPlan::FullScan { .. }) => {
                match *input {
                    PhysicalPlan::FullScan { table, columns } => {
                        PhysicalPlan::ZoneMapScan { table, columns, predicate }
                    }
                    other => PhysicalPlan::Filter { predicate, input: Box::new(other) },
                }
            }
            other => other,
        })
    }
}


struct EliminateTrueFilter;

impl OptimizerRule for EliminateTrueFilter {
    fn name(&self) -> &str { "eliminate_true_filter" }

    fn apply(&self, plan: PhysicalPlan) -> PhysicalPlan {
        rewrite(plan, &|node| match node {
            PhysicalPlan::Filter {
                predicate: PhysicalExpr::Literal(LiteralValue::Bool(true)),
                input,
            } => *input,
            other => other,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical_plan::physical_operators::{AggFunc, AggSpec, CmpOp, LiteralValue, PhysicalExpr, PhysicalPlan};

    fn col(name: &str) -> PhysicalExpr {
        PhysicalExpr::Column(name.to_string())
    }

    fn scan(table: &str, columns: Vec<&str>) -> PhysicalPlan {
        PhysicalPlan::FullScan {
            table: table.to_string(),
            columns: columns.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    // Filter + Scan → ZoneMapScan
    #[test]
    fn test_predicate_pushdown() {
        let predicate = PhysicalExpr::Compare {
            left: Box::new(col("age")),
            op: CmpOp::Gt,
            right: Box::new(PhysicalExpr::Literal(LiteralValue::Int(30))),
        };

        let plan = PhysicalPlan::Filter {
            predicate,
            input: Box::new(scan("users", vec!["age", "name"])),
        };

        let optimized = Optimizer::new().optimize(plan);
        println!("{}", optimized);

        match optimized {
            PhysicalPlan::ZoneMapScan { table, columns, predicate } => {
                assert_eq!(table, "users");
                assert_eq!(columns, vec!["age", "name"]);
                match predicate {
                    PhysicalExpr::Compare { left, op, right } => {
                        match (*left, op, *right) {
                            (
                                PhysicalExpr::Column(col),
                                CmpOp::Gt,
                                PhysicalExpr::Literal(LiteralValue::Int(30)),
                            ) => assert_eq!(col, "age"),
                            _ => panic!("unexpected predicate shape"),
                        }
                    }
                    _ => panic!("expected BinaryOp predicate"),
                }
            }
            _ => panic!("expected ZoneMapScan after pushdown"),
        }
    }

    // Filter { predicate: true } → child node directly
    #[test]
    fn test_eliminate_true_filter() {
        let plan = PhysicalPlan::Filter {
            predicate: PhysicalExpr::Literal(LiteralValue::Bool(true)),
            input: Box::new(scan("users", vec!["name"])),
        };

        let optimized = Optimizer::new().optimize(plan);
        println!("{}", optimized);

        match optimized {
            PhysicalPlan::FullScan { .. } => {}
            _ => panic!("expected Filter to be eliminated, leaving Scan"),
        }
    }

    // Filter over Aggregate should not be pushed down
    #[test]
    fn test_no_pushdown_over_aggregate() {
        let plan = PhysicalPlan::Filter {
            predicate: PhysicalExpr::Compare {
                left: Box::new(col("age")),
                op: CmpOp::Gt,
                right: Box::new(PhysicalExpr::Literal(LiteralValue::Int(30))),
            },
            input: Box::new(PhysicalPlan::Aggregate {
                group_by: vec![col("name")],
                aggregates: vec![AggSpec {
                    func: AggFunc::Sum,
                    arg: col("age"),
                    output_name: "sum(age)".to_string(),
                }],
                input: Box::new(scan("users", vec!["age", "name"])),
            }),
        };

        let optimized = Optimizer::new().optimize(plan);
        println!("{}", optimized);

        match optimized {
            PhysicalPlan::Filter { input, .. } => match *input {
                PhysicalPlan::Aggregate { .. } => {}
                _ => panic!("expected Aggregate under Filter"),
            },
            _ => panic!("expected Filter to remain"),
        }
    }
}
