use std::fmt;

#[derive(Clone)]
pub enum LiteralValue {
    I8(i8),   I16(i16), I32(i32), I64(i64),
    U8(u8),   U16(u16), U32(u32), U64(u64),
    F32(f32), F64(f64),
    Str(String),
    Bool(bool),
    Null,
}

pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug)]
pub enum AggFunc {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

// PhysicalExpr intentionally has no Aggregate variant.
// At the physical layer, aggregates are stateful operators — they accumulate state
// across multiple batches and cannot be evaluated like an expression.
// Instead, aggregate functions are represented as AggSpec on PhysicalPlan::Aggregate.
// By the time downstream operators (e.g. Project) see the data, the aggregate result
// is already materialized as a named column in the batch — referenced as Column("sum(age)").
pub enum PhysicalExpr {
    Column(String),
    Literal(LiteralValue),
    Compare {
        left: Box<PhysicalExpr>, 
        op: CmpOp, 
        right: Box<PhysicalExpr>,
    },
    Logical {
        left: Box<PhysicalExpr>,
        op: LogicalOp,
        right: Box<PhysicalExpr>,
    }
}

pub struct AggSpec {
    pub func: AggFunc,
    pub arg: PhysicalExpr,
    pub output_name: String,
}

pub enum PhysicalPlan {
    FullScan {
        table: String,
        columns: Vec<String>,
    },
    ZoneMapScan {
        table: String,
        columns: Vec<String>,
        predicate: PhysicalExpr,
    },
    Filter {
        predicate: PhysicalExpr,
        input: Box<PhysicalPlan>,
    },
    Project {
        projections: Vec<PhysicalExpr>,
        input: Box<PhysicalPlan>,
    },
    Aggregate {
        group_by: Vec<PhysicalExpr>,
        aggregates: Vec<AggSpec>,
        input: Box<PhysicalPlan>,
    },
    Limit {
        limit: u64,
        input: Box<PhysicalPlan>,
    },
}


impl PhysicalPlan {
    pub fn children(&self) -> Vec<&PhysicalPlan> {
        match self {
            PhysicalPlan::FullScan { .. }     => vec![],
            PhysicalPlan::ZoneMapScan { .. }  => vec![],
            PhysicalPlan::Filter { input, .. }    => vec![input],
            PhysicalPlan::Project { input, .. }   => vec![input],
            PhysicalPlan::Aggregate { input, .. } => vec![input],
            PhysicalPlan::Limit { input, .. }     => vec![input],
        }
    }

    pub fn with_new_children(self, mut new_children: Vec<PhysicalPlan>) -> PhysicalPlan {
        match self {
            PhysicalPlan::FullScan { .. }    => self,
            PhysicalPlan::ZoneMapScan { .. } => self,
            PhysicalPlan::Filter { predicate, .. } => PhysicalPlan::Filter {
                predicate,
                input: Box::new(new_children.remove(0)),
            },
            PhysicalPlan::Project { projections, .. } => PhysicalPlan::Project {
                projections,
                input: Box::new(new_children.remove(0)),
            },
            PhysicalPlan::Aggregate { group_by, aggregates, .. } => PhysicalPlan::Aggregate {
                group_by,
                aggregates,
                input: Box::new(new_children.remove(0)),
            },
            PhysicalPlan::Limit { limit, .. } => PhysicalPlan::Limit {
                limit,
                input: Box::new(new_children.remove(0)),
            },
        }
    }
}

impl fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::I8(v)  => write!(f, "{}", v),
            LiteralValue::I16(v) => write!(f, "{}", v),
            LiteralValue::I32(v) => write!(f, "{}", v),
            LiteralValue::I64(v) => write!(f, "{}", v),
            LiteralValue::U8(v)  => write!(f, "{}", v),
            LiteralValue::U16(v) => write!(f, "{}", v),
            LiteralValue::U32(v) => write!(f, "{}", v),
            LiteralValue::U64(v) => write!(f, "{}", v),
            LiteralValue::F32(v) => write!(f, "{}", v),
            LiteralValue::F64(v) => write!(f, "{}", v),
            LiteralValue::Str(s) => write!(f, "'{}'", s),
            LiteralValue::Bool(b)=> write!(f, "{}", b),
            LiteralValue::Null   => write!(f, "NULL"),
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            CmpOp::Eq    => "=",
            CmpOp::NotEq => "!=",
            CmpOp::Lt    => "<",
            CmpOp::LtEq  => "<=",
            CmpOp::Gt    => ">",
            CmpOp::GtEq  => ">=",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            LogicalOp::And    => "AND",
            LogicalOp::Or     => "OR",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for AggFunc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AggFunc::Count => "COUNT",
            AggFunc::Sum   => "SUM",
            AggFunc::Avg   => "AVG",
            AggFunc::Min   => "MIN",
            AggFunc::Max   => "MAX",
        };
        write!(f, "{}", s)
    }
}

impl fmt::Display for PhysicalExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhysicalExpr::Column(col)              => write!(f, "{}", col),
            PhysicalExpr::Literal(lit)             => write!(f, "{}", lit),
            PhysicalExpr::Compare { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            PhysicalExpr::Logical { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
        }
    }
}

impl fmt::Display for PhysicalPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl PhysicalPlan {
    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match self {
            PhysicalPlan::FullScan { table, columns } => {
                writeln!(f, "{}FullScan({}, cols=[{}])", indent, table, columns.join(", "))
            }
            PhysicalPlan::ZoneMapScan { table, columns, predicate } => {
                writeln!(f, "{}ZoneMapScan({}, cols=[{}], predicate={})", indent, table, columns.join(", "), predicate)
            }
            PhysicalPlan::Filter { predicate, input } => {
                writeln!(f, "{}Filter({})", indent, predicate)?;
                input.fmt_indented(f, depth + 1)
            }
            PhysicalPlan::Project { projections, input } => {
                let cols: Vec<String> = projections.iter().map(|e| e.to_string()).collect();
                writeln!(f, "{}Project({})", indent, cols.join(", "))?;
                input.fmt_indented(f, depth + 1)
            }
            PhysicalPlan::Aggregate { group_by, aggregates, input } => {
                let gb: Vec<String>  = group_by.iter().map(|e| e.to_string()).collect();
                let agg: Vec<String> = aggregates.iter().map(|a| format!("{}({}) -> {}", a.func, a.arg, a.output_name)).collect();
                writeln!(f, "{}HashAggregate(group_by=[{}], aggs=[{}])", indent, gb.join(", "), agg.join(", "))?;
                input.fmt_indented(f, depth + 1)
            }
            PhysicalPlan::Limit { limit, input } => {
                writeln!(f, "{}Limit({})", indent, limit)?;
                input.fmt_indented(f, depth + 1)
            }
        }
    }
}
