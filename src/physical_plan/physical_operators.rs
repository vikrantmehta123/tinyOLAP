pub enum LiteralValue {
    Int(i64),
    Float(f64),
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

// PhysicalExpr intentionally has no Aggregate variant.
// At the physical layer, aggregates are stateful operators — they accumulate state
// across multiple batches and cannot be evaluated like an expression.
// Instead, aggregate functions are represented as AggSpec on PhysicalPlan::Aggregate.
// By the time downstream operators (e.g. Project) see the data, the aggregate result
// is already materialized as a named column in the batch — referenced as Column("sum(age)").
pub enum PhysicalExpr {
    Column(String),
    Literal(LiteralValue),
    BinaryOp {
        left: Box<PhysicalExpr>,
        op: BinaryOp,
        right: Box<PhysicalExpr>,
    },
}

pub struct AggSpec {
    pub func: AggFunc,
    pub arg: PhysicalExpr,
    pub output_name: String,
}

pub enum PhysicalPlan {
    Scan {
        table: String,
        columns: Vec<String>,
    },
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
