# tinyOLAP — Query Pipeline Refactor

## Context

Read CLAUDE.md, SPEC.md, and the existing source under `src/` before starting.
Pay particular attention to:
- `src/parser/` — existing SQL parsing code
- `src/analyser.rs` — existing semantic analysis
- `src/processors/` — existing query pipeline stages
- `src/executor.rs` — existing execution entry point

The existing parser, analyser, and executor are going to be restructured and
replaced by the pipeline described below. The storage layer (`src/storage/`),
encoding layer (`src/encoding/`), and `src/main.rs` REPL should not be touched.

---

## Goal

Introduce a proper query compilation pipeline with clear separation between stages:

```
SQL string
    │
    ▼
frontend/parser.rs      ← parse SQL text → sqlparser AST
    │
    ▼
frontend/validator.rs   ← shape validation (rejects unsupported SQL structure)
    │
    ▼
frontend/analyzer.rs    ← schema validation (resolves names, checks types)
    │                      returns Result<(), String> — no new output type
    ▼
plan/lower.rs           ← sqlparser AST → LogicalPlan (name resolution happens here too)
    │
    ▼
plan/optimizer.rs       ← rule-based logical optimizations
    │
    ▼
physical/lower.rs       ← LogicalPlan → PhysicalPlan (stubs for now)
```

---

## Target Directory Structure

Introduce the following new modules under `src/`. Do not delete the existing
`processors/` or `executor.rs` yet — they will be removed in a later task once
the new pipeline is wired end-to-end.

```
src/
  frontend/
    mod.rs
    parser.rs       ← thin wrapper around sqlparser
    validator.rs    ← shape validation (rejects unsupported SQL structure)
    analyzer.rs     ← schema validation only — checks names and types exist,
                       returns Result<(), String>, produces no new type

  logical-plan/
    mod.rs
    logical.rs      ← LogicalPlan, LogicalExpr, and shared expr types
                       (LiteralValue, BinaryOp, AggFunc, DataType)
    lower.rs        ← sqlparser AST → LogicalPlan
                       does name resolution and structural transformation together
    optimizer.rs    ← rule-based rewrites on LogicalPlan

  physical-plan/
    mod.rs
    physical.rs     ← PhysicalPlan, PhysicalExpr (stubs — see below)
    lower.rs        ← LogicalPlan → PhysicalPlan (stub)
```

---

## Hardcoded Schema (for now)

tinyOLAP currently supports a single table loaded from `schema.json` at startup. For reference, you can look at `data/tinyolap_smoke/schema.json`

The schema provides:
- table name (a `String`)
- columns: a list of `(column_name: String, data_type: DataType)`

`DataType` should be defined in `plan/logical.rs` since it is used by both the
analyzer (for type checking) and the lowerer (for fully-qualified column nodes).

Define a simple `TableSchema` struct — either reuse one from the existing storage
layer if suitable, or define a new one in `plan/logical.rs`:

```rust
pub struct TableSchema {
    pub name:    String,
    pub columns: Vec<ColumnSchema>,
}
pub struct ColumnSchema {
    pub name:      String,
    pub data_type: DataType,
}
```

---

## Stage 1 — frontend/parser.rs

A thin wrapper. No logic here beyond calling sqlparser.

```rust
pub fn parse(sql: &str) -> Result<Vec<Statement>, String>
```

Use `GenericDialect`. Return a `Vec<Statement>` or a string error.

---

## Stage 2 — frontend/validator.rs

Shape validation — rejects SQL that tinyOLAP does not support.
This runs before the analyzer. It does NOT touch the catalog.

Supported SQL shape for SELECT:
```
SELECT <col | aggregate | *>, ...
FROM <single table>
[WHERE <predicate>]
[GROUP BY <col>, ...]
[LIMIT <n>]
```

Reject everything else with a descriptive error string:
- `WITH` / CTEs
- `UNION`, `INTERSECT`, `EXCEPT`
- `DISTINCT`
- `JOIN` of any kind
- Multiple tables in FROM
- Subqueries anywhere (in SELECT list, FROM, WHERE)
- `HAVING`
- `OFFSET`
- `ORDER BY` (defer to later)
- Any operator in WHERE other than: `=`, `!=`, `<`, `<=`, `>`, `>=`, `AND`, `OR`
- Any function in SELECT other than: `COUNT`, `SUM`, `AVG`, `MIN`, `MAX`
- Any function in WHERE

For INSERT, accept it without detailed validation for now — the existing
insert path will remain unchanged.

Public API:
```rust
pub fn validate(stmt: &Statement) -> Result<(), String>
```

Use plain recursive functions + `Result` propagation via `?`. No Visitor needed. There should be one public function and internally it should call the other validation functions.

---

## Stage 3 — frontend/analyzer.rs

Schema validation only — checks that names and types are valid against the schema.
Produces no new type. Returns `Result<(), String>`.

```rust
pub fn analyze(stmt: &Statement, schema: &TableSchema) -> Result<(), String>
```

The analyzer must:
1. Verify the FROM table name matches `schema.name`
2. Verify every bare `Identifier` column reference exists in `schema.columns`
3. Verify every qualified `CompoundIdentifier` — table part matches schema name, column part exists
4. Verify types are compatible in `BinaryOp` expressions (e.g. comparing a string column to an integer literal should be rejected)
5. Verify `SELECT *` is always valid (no column checking needed)

While analysing, either we can use the visitor provided by the `sqlparser` crate to collect the columns, etc. Or we can write our own, whichever is easier.

---

## Stage 4 — plan/logical.rs

The logical plan is a **tree** — each node has children via `Box<LogicalPlan>`.

Define all shared expression types here — these are used by both `lower.rs`
and `optimizer.rs`:

```rust
pub enum DataType { Int64, Float64, Utf8, Boolean }

pub enum LiteralValue { Int(i64), Float(f64), Str(String), Bool(bool), Null }

pub enum BinaryOp { Eq, NotEq, Lt, LtEq, Gt, GtEq, And, Or }

pub enum AggFunc { Count, Sum, Avg, Min, Max }

pub enum LogicalExpr {
    Column   (String, String),        // (table, column) — always fully qualified
    Literal  (LiteralValue),
    BinaryOp { left: Box<LogicalExpr>, op: BinaryOp, right: Box<LogicalExpr> },
    Aggregate{ func: AggFunc, arg: Box<LogicalExpr> },
}

pub enum LogicalPlan {
    Scan      { table: String },
    Filter    { predicate: LogicalExpr, input: Box<LogicalPlan> },
    Project   { projections: Vec<LogicalExpr>, input: Box<LogicalPlan> },
    Aggregate { group_by: Vec<LogicalExpr>, aggregates: Vec<LogicalExpr>, input: Box<LogicalPlan> },
    Limit     { limit: u64, input: Box<LogicalPlan> },
}
```

Implement two methods on `LogicalPlan` to enable tree traversal:

```rust
impl LogicalPlan {
    // references to child nodes
    pub fn children(&self) -> Vec<&LogicalPlan>

    // reconstruct this node with replacement children
    // used by the rewrite engine to rebuild the tree after rewriting children
    pub fn with_new_children(self, new_children: Vec<LogicalPlan>) -> LogicalPlan
}
```

---

## Stage 5 — plan/lower.rs

Converts a validated sqlparser `Statement` → `LogicalPlan`.
This is where name resolution and structural transformation happen together.

```rust
pub fn lower(stmt: &Statement, schema: &TableSchema) -> Result<LogicalPlan, String>
```

The lowerer does two things simultaneously:
- **Resolves names** — bare `Identifier("age")` becomes `LogicalExpr::Column("users", "age")`
  by looking up the column in the schema. Qualified `CompoundIdentifier` is verified and used directly.
- **Builds the plan tree** bottom-up:
  - Start with `LogicalPlan::Scan { table }`
  - Wrap with `Filter` if there is a WHERE clause
  - Wrap with `Aggregate` if there are GROUP BY columns or aggregate functions in projections
  - Wrap with `Project` for the SELECT list
  - Wrap with `Limit` if present

`SELECT *` expands to all columns in the schema as `LogicalExpr::Column` nodes.

Use plain recursive functions. No Visitor.

---

## Stage 6 — plan/optimizer.rs

Rule-based rewrites on the logical plan tree.

### The rewrite engine

```rust
// Applies a rule bottom-up to every node in the tree.
// Children are rewritten before the rule is applied to the parent.
pub fn rewrite<F>(plan: LogicalPlan, rule: &F) -> LogicalPlan
where
    F: Fn(LogicalPlan) -> LogicalPlan
```

Bottom-up means: recurse into children first, then apply the rule to the current node.
This ensures that when a rule sees a node, its children are already fully rewritten.
A rule can eliminate a node by returning the node's child instead of the node itself.

### Rules to implement

**Rule 1 — Constant folding** (`rule_fold_constants`)

Applies to `Filter` and `Project` nodes. Walks their expressions and evaluates
any `BinaryOp` where both sides are `Literal`:

- `Int(a) op Int(b)` → `Bool(result)` for comparison ops
- `Bool(true) AND x` → `x`
- `Bool(false) AND x` → `Bool(false)`
- `Bool(true) OR x` → `Bool(true)`
- `Bool(false) OR x` → `x`

**Rule 2 — Eliminate always-true filters** (`rule_eliminate_true_filter`)

```
Filter { predicate: Literal(Bool(true)), input } → *input
```

The `Filter` node is dropped entirely; its child takes its place.
Run this after constant folding so it catches filters that folded to `true`.

### The optimizer entry point

```rust
pub fn optimize(plan: LogicalPlan) -> LogicalPlan {
    let rules: Vec<fn(LogicalPlan) -> LogicalPlan> = vec![
        rule_fold_constants,
        rule_eliminate_true_filter,
    ];
    rules.into_iter().fold(plan, |p, rule| rewrite(p, &rule))
}
```

---

## Stage 7 — physical/physical.rs (stubs)

Define the physical plan types as stubs. No real implementation yet —
the physical execution will be wired in a later task.

```rust
pub enum PhysicalPlan {
    SequentialScan {
        table:     String,
        predicate: Option<PhysicalExpr>,
    },
    Filter {
        predicate: PhysicalExpr,
        input:     Box<PhysicalPlan>,
    },
    Project {
        columns: Vec<PhysicalExpr>,
        input:   Box<PhysicalPlan>,
    },
    HashAggregate {
        group_by:   Vec<PhysicalExpr>,
        aggregates: Vec<PhysicalExpr>,
        input:      Box<PhysicalPlan>,
    },
    Limit {
        limit: u64,
        input: Box<PhysicalPlan>,
    },
}

pub enum PhysicalExpr {
    Column   (String, String),
    Literal  (LiteralValue),
    BinaryOp { left: Box<PhysicalExpr>, op: BinaryOp, right: Box<PhysicalExpr> },
    Aggregate{ func: AggFunc, arg: Box<PhysicalExpr> },
}
```

Implement `children()` and `with_new_children()` on `PhysicalPlan` mirroring
the logical plan — you will need them for physical optimizations later.

---

Re-export or reuse `LiteralValue`, `BinaryOp`, `AggFunc` from `plan/logical.rs`
rather than redefining them.

## Stage 8 — physical/lower.rs (stub)

```rust
pub fn lower_to_physical(plan: LogicalPlan) -> PhysicalPlan
```

Mechanical 1:1 mapping for now:
- `LogicalPlan::Scan`      → `PhysicalPlan::SequentialScan { predicate: None }`
- `LogicalPlan::Filter`    → `PhysicalPlan::Filter`
- `LogicalPlan::Project`   → `PhysicalPlan::Project`
- `LogicalPlan::Aggregate` → `PhysicalPlan::HashAggregate`
- `LogicalPlan::Limit`     → `PhysicalPlan::Limit`

Recurse into children via plain recursive calls.

---

## Key Invariants to Enforce

- `frontend/analyzer.rs` is the only file that does schema validation against names.
- `frontend/validator.rs` does NOT touch the schema — shape only.
- `logical-plan/lower.rs` is the only place sqlparser AST nodes are converted to LogicalPlan
  nodes.
- Each stage communicates with the next through a clean boundary:
  `sqlparser::Statement` → (validator + analyzer check it) → `logical-plan::lower` → `LogicalPlan` → `PhysicalPlan`

---

## What NOT to Do

- Do not wire the new pipeline into `executor.rs` or `main.rs` yet. We are changing a lot, and quite likely the executor will also change. That's okay. We can plan for the execution change later.
- Do not implement physical execution (the `PhysicalNode` trait, `next_batch`, etc.)
  — that is a future task.
- Do not implement physical optimizations (predicate pushdown into scan, index scan
  selection) — future task.
- Do not add `ORDER BY` support — future task.

---

## Tests

Write unit tests in each module covering at minimum:

**frontend/validator.rs**
- A valid SELECT passes
- GROUP BY passes
- LIMIT passes
- WITH/CTE is rejected
- JOIN is rejected
- Subquery in WHERE is rejected
- Unknown operator in WHERE is rejected

**frontend/analyzer.rs**
- Valid column reference passes
- Unknown column is rejected
- Unknown table is rejected
- Qualified column reference with wrong table name is rejected
- Type mismatch in WHERE (string column compared to integer literal) is rejected
- `SELECT *` always passes

**logical-plan/optimizer.rs**
- `1 = 1 AND age > 30` folds to `age > 30`
- `WHERE true` filter node is eliminated
- `WHERE false` filter node is kept
- Folding does not affect non-constant expressions

---

## Style Notes (from CLAUDE.md)

- Use `Result`/`Option` over panics
- Prefer safe Rust; no `unsafe`
- Keep it simple — no premature abstractions
- Stick to SOLID principles — each file has one clear responsibility
- Tests are good when they lock in correctness of a tricky piece