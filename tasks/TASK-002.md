# TASK-002 — Arrow-backed Physical Execution

## Goal

Build the execution layer for tinyOLAP using **Apache Arrow** as the in-memory data format. After this task, the REPL can take a SQL string, run it end-to-end through the existing logical and physical planning pipeline, execute the physical plan with vectorized Arrow operators, and return rows.

Performance is the bar.

---

## In Scope

### Operators (all four required)
1. **`FullScanExec`** — materializes one Arrow `RecordBatch` per part on disk. Discovers `part_*` directories under the table dir at construction. One batch per part, no granule skipping.
2. **`FilterExec`** — evaluates a predicate against each batch, produces an Arrow `BooleanArray` mask, applies `arrow::compute::filter_record_batch` to mask all columns in one SIMD pass. Empty batches are skipped (the operator loops to maintain a non-empty output contract). As we do this, we need to understand an example or two about how Arrow internally computes this.
3. **`ProjectExec`** — selects and reorders columns. Uses `RecordBatch::project(indices)` for zero-copy column selection. Bare column references only — expression projections (`SELECT age + 1`) are out of scope for this task and the repo.
4. **`LimitExec`** — streaming limit. Pulls batches until the cumulative row count reaches the limit, slices the crossing batch via `Array::slice(offset, len)` (zero-copy, refcounted), then returns `None`.

### Expression evaluation
- Returns `arrow::array::ColumnarValue` (`Array | Scalar`). Literals stay scalar; never broadcast.
- `Compare` dispatches to Arrow's comparison kernels — `gt_scalar`, `eq`, `lt`, etc. — picking the scalar-broadcast variant when one side is a `Scalar`.
- `Logical` (AND/OR) dispatches to `arrow::compute::kernels::boolean::{and, or}`.
- Type coercion is out of scope. Same-type pairs only; mismatched types error at evaluation time. The lowerer is responsible for inserting casts later (separate task).

### Builder
- `pub fn build(plan: PhysicalPlan, schema: &TableSchema, table_dir: &Path) -> Result<Box<dyn ExecutionPlan>, ExecutionError>`
- Recursive: builds children before wrapping. One match arm per `PhysicalPlan` variant.
- `ZoneMapScan` and `Aggregate` variants are out of scope — they remain `unimplemented!` for now. Either disable the `PredicatePushdown` optimizer rule or have it skip producing `ZoneMapScan` until TASK-003. **Decision: disable the rule for this task.**

### Storage layer changes
- `ColumnChunk` enum is **removed**. Replaced everywhere by `Arc<dyn Array>` (Arrow's `ArrayRef`).
- `ColumnReader::read_all()` and `StringColumnReader::read_all()` return Arrow arrays directly (`Int64Array`, `StringArray`, etc., wrapped in `ArrayRef`).
- `TableWriter::insert(...)` signature changes to accept Arrow arrays. Existing INSERT integration points get updated to match.

### REPL integration
- `src/main.rs` wires up: parse → validate → analyze → lower → optimize → build → drive.
- Old `src/processors/` and `src/executor.rs` are deleted once the new pipeline produces equivalent output.
- A single drive function loops `next_batch()` and prints / collects results.

### Tests
- Unit tests per operator where unit-testable without disk (FilterExec, ProjectExec, LimitExec — use a `VecBatchExec` test fixture that yields preset Arrow batches).
- One end-to-end integration test: a known query against `data/tinyolap_smoke/` returns the expected rows.
- Optional micro-benchmarks for FilterExec and FullScanExec (criterion).

---

## Out of Scope (deferred)

- **`HashAggregateExec`** — TASK-003 (its own task; biggest operator, hash strategy + state aggregators deserve dedicated scope).
- **`ZoneMapScanExec`, `IndexScanExec`** — TASK-004 (predicate pushdown using `.mrk` zone-map metadata).
- **Expression projections** — `SELECT age + 1`. Requires generalizing the evaluator beyond column references in projections.
- **Type coercion** — `I32 col vs Int literal`. Requires a cast pass in the lowerer.
- **Async streams** — sync pull (`next_batch`) is fine for now.
- **Parquet adoption** — our `.bin`/`.mrk` format stays.
- **Multi-table catalog** — single table.
- **ORDER BY, GROUP BY (other than for aggregation), DISTINCT, JOIN, CTE, subqueries** — already excluded by the validator.
- **Schema in `Batch`** — relying on `RecordBatch::schema()` directly; no wrapper type.

---

## Decisions Made

- **Crate:** `arrow` (the official Apache crate, latest). Not `arrow2`.
- **Roll-back state:** uncommitted work since `0dff3b1` is rolled back. We rebuild on Arrow from a clean baseline. The `BinaryOp` split (`CmpOp`/`LogicalOp`) survives because it's strictly cleaner; reapply that as the first commit in this task.
- **`PredicatePushdown` rule:** disabled until TASK-004 wires `ZoneMapScanExec`.
- **Empty-batch contract:** operators emit non-empty batches only. `FilterExec` loops; `LimitExec` returns `None` early.

---

## Subtask Order (smallest blast radius first)

1. **Roll back** uncommitted work to `0dff3b1`. Reapply the `CmpOp`/`LogicalOp` split as a fresh commit (no executor code yet).
2. **Add `arrow` dependency** in `Cargo.toml`. Confirm `cargo build` is green.
3. **Replace `ColumnChunk` with `ArrayRef` throughout storage.** Update `ColumnReader` and `StringColumnReader` to produce Arrow arrays. Update `TableWriter` to consume Arrow arrays. This is the largest mechanical change — do it as one commit and verify INSERT still works end-to-end.
4. **Define `ExecutionPlan` trait + `ExecutionError`** in `src/execution/executor.rs`. Use `arrow::record_batch::RecordBatch` directly; no `Batch` wrapper.
5. **`FullScanExec`** — disk parts → Arrow `RecordBatch`. One batch per part.
6. **`LimitExec`** — uses `RecordBatch::slice` (or per-column `Array::slice`) for the crossing batch.
7. **`expr.rs`** — `evaluate(&PhysicalExpr, &RecordBatch) -> Result<ColumnarValue, ExecutionError>`. Scalar literals, vector columns, Arrow kernels.
8. **`FilterExec`** — `filter_record_batch` after evaluating the predicate.
9. **`ProjectExec`** — `RecordBatch::project(indices)` after resolving projection names to column indices once at construction.
10. **`builder.rs`** — recursive build matching `PhysicalPlan` variants.
11. **REPL wiring in `main.rs`** + delete `src/processors/` and `src/executor.rs`.
12. **End-to-end test** against `data/tinyolap_smoke/`. Verify rows and counts.
13. **Display impls** for the executor tree and `RecordBatch` printing (already-Arrow has `pretty::print_batches`; use it).

---

## Success Criteria

- `cargo test` is green, including the end-to-end integration test.
- Running `cargo run` and entering a SELECT statement against a populated table prints matching rows.
- Old `src/processors/` and `src/executor.rs` are deleted.
- No `unimplemented!` paths reachable for any SELECT shape that the validator accepts (modulo aggregation, deferred to TASK-003).
- A `criterion` benchmark exists for `FilterExec` and demonstrates SIMD acceleration vs a naive baseline (optional but encouraged for portfolio value).

---

## Key Files Touched

- `Cargo.toml` — add `arrow`
- `src/storage/column_chunk.rs` — delete (or replace with thin Arrow re-exports)
- `src/storage/column_reader.rs` — return Arrow arrays
- `src/storage/string_column_reader.rs` — return Arrow arrays
- `src/storage/table_writer.rs` — accept Arrow arrays
- `src/execution/` — all new
  - `executor.rs` — trait + error
  - `full_scan.rs`
  - `filter.rs`
  - `project.rs`
  - `limit.rs`
  - `expr.rs`
  - `builder.rs`
  - `mod.rs`
- `src/main.rs` — REPL wiring
- Delete: `src/processors/`, `src/executor.rs`

---

## References to Read Before Starting

- `arrow-rs` book: <https://docs.rs/arrow/latest/arrow/> — focus on `RecordBatch`, `ArrayRef`, `compute::kernels`, `ColumnarValue`
- DataFusion `physical-expr/src/expressions/binary.rs` — the dispatch table for `(left, op, right)` over `ColumnarValue`
- DataFusion `physical-plan/src/filter.rs` — `FilterExec` shape
- DataFusion `physical-plan/src/projection.rs` — `ProjectionExec` shape
