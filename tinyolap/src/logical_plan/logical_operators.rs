//! Defines the Logical Plan Operators.
//! These operators will be used when defining a query plan for
//! query processing.

use crate::catalog::schema::DataType;
use std::fmt;

pub enum LiteralValue {
    Int(i64),   // No need to differentiate between 18, i16, i32, i64 at logical plan
    Float(f64), // Same f32, etc. Don't need it in Logical Plan
    Str(String),
    Bool(bool),
    Null,
}

pub enum BinaryOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    And,
    Or,
}

pub enum AggFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

pub enum LogicalExpr {
    Column(String, String), // (table, column) — Lowering will ensure this
    Literal(LiteralValue),
    BinaryOp {
        left: Box<LogicalExpr>,
        op: BinaryOp,
        right: Box<LogicalExpr>,
    },
    Aggregate {
        func: AggFunc,
        arg: Box<LogicalExpr>,
    },
    Cast {
        expr: Box<LogicalExpr>, 
        target_datatype: DataType,
    }
}

// Each variant is a node in the plan tree. Data flows bottom-up.
pub enum LogicalPlan {
    Scan {
        table: String,
    },
    Filter {
        predicate: LogicalExpr,
        input: Box<LogicalPlan>,
    }, // 'input' is child for the plan
    Project {
        projections: Vec<LogicalExpr>,
        input: Box<LogicalPlan>,
    },
    Aggregate {
        group_by: Vec<LogicalExpr>,
        aggregates: Vec<LogicalExpr>,
        input: Box<LogicalPlan>,
    },
    Limit {
        limit: u64,
        input: Box<LogicalPlan>,
    },
}

impl LogicalPlan {
    // Returns references to child nodes. Used by the optimizer to traverse the tree.
    pub fn children(&self) -> Vec<&LogicalPlan> {
        match self {
            LogicalPlan::Scan { .. } => vec![],
            LogicalPlan::Filter { input, .. } => vec![input],
            LogicalPlan::Project { input, .. } => vec![input],
            LogicalPlan::Aggregate { input, .. } => vec![input],
            LogicalPlan::Limit { input, .. } => vec![input],
        }
    }

    // Rebuilds this node with rewritten children.
    // Used when rewriting the Logical Plan using OptimizerRules
    pub fn with_new_children(self, mut new_children: Vec<LogicalPlan>) -> LogicalPlan {
        match self {
            LogicalPlan::Scan { .. } => self,
            LogicalPlan::Filter { predicate, .. } => LogicalPlan::Filter {
                predicate,
                input: Box::new(new_children.remove(0)),
            },
            LogicalPlan::Project { projections, .. } => LogicalPlan::Project {
                projections,
                input: Box::new(new_children.remove(0)),
            },
            LogicalPlan::Aggregate {
                group_by,
                aggregates,
                ..
            } => LogicalPlan::Aggregate {
                group_by,
                aggregates,
                input: Box::new(new_children.remove(0)),
            },
            LogicalPlan::Limit { limit, .. } => LogicalPlan::Limit {
                limit,
                input: Box::new(new_children.remove(0)),
            },
        }
    }
}

impl fmt::Display for LogicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl LogicalPlan {
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match self {
            LogicalPlan::Scan { table } => {
                writeln!(f, "{}Scan({})", indent, table)
            }
            LogicalPlan::Filter { predicate, input } => {
                writeln!(f, "{}Filter({})", indent, predicate)?;
                input.fmt_indented(f, depth + 1)
            }
            LogicalPlan::Project { projections, input } => {
                let cols: Vec<String> = projections.iter().map(|e| e.to_string()).collect();
                writeln!(f, "{}Project({})", indent, cols.join(", "))?;
                input.fmt_indented(f, depth + 1)
            }
            LogicalPlan::Aggregate {
                group_by,
                aggregates,
                input,
            } => {
                let gb: Vec<String> = group_by.iter().map(|e| e.to_string()).collect();
                let agg: Vec<String> = aggregates.iter().map(|e| e.to_string()).collect();
                writeln!(
                    f,
                    "{}Aggregate(group_by=[{}], aggs=[{}])",
                    indent,
                    gb.join(", "),
                    agg.join(", ")
                )?;
                input.fmt_indented(f, depth + 1)
            }
            LogicalPlan::Limit { limit, input } => {
                writeln!(f, "{}Limit({})", indent, limit)?;
                input.fmt_indented(f, depth + 1)
            }
        }
    }
}

impl fmt::Display for LogicalExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogicalExpr::Column(table, col) => write!(f, "{}.{}", table, col),
            LogicalExpr::Literal(lit) => write!(f, "{}", lit),
            LogicalExpr::BinaryOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            LogicalExpr::Aggregate { func, arg } => write!(f, "{}({})", func, arg),
            LogicalExpr::Cast { expr, target_datatype} => write!(f, "CAST({} AS {:?})", expr, target_datatype),
        }
    }
}

impl fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::Int(i) => write!(f, "{}", i),
            LiteralValue::Float(v) => write!(f, "{}", v),
            LiteralValue::Str(s) => write!(f, "'{}'", s),
            LiteralValue::Bool(b) => write!(f, "{}", b),
            LiteralValue::Null => write!(f, "NULL"),
        }
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Eq => "=",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for AggFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AggFunc::Count => "COUNT",
            AggFunc::Sum => "SUM",
            AggFunc::Avg => "AVG",
            AggFunc::Min => "MIN",
            AggFunc::Max => "MAX",
        };
        write!(f, "{}", s)
    }
}
