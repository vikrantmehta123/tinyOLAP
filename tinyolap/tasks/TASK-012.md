# TASK-004 — Sorted / Streaming Aggregation

## Goal

Add a second aggregation strategy — `SortedAggregateExec` — that aggregates in **O(1) memory** when the `GROUP BY` columns are a **prefix of the table's sort key**. Plus the planner rule that dispatches between `HashAggregateExec` (general case) and `SortedAggregateExec` (sort-aligned case).

This is the operator that makes tinyOLAP competitive with ClickHouse on sort-key-aligned aggregations. Hash aggregation handles every query correctly; sorted aggregation handles a *subset* of queries dramatically faster.

## Prerequisite

- **TASK-003 (HashAggregateExec) must be complete.** This task adds a second strategy alongside the first; it doesn't replace it.
- All aggregate functions (`COUNT`, `SUM`, `AVG`, `MIN`, `MAX`) and their type rules from TASK-003 apply unchanged. Both strategies produce the same output for the same input.

---

## In Scope

### The streaming algorithm

For a sort-key-aligned aggregation:

1. Pull batches from the child in order.
2. Within each batch, rows are already grouped contiguously (because they're sorted on the group-by columns).
3. Maintain **one running accumulator state**, plus a "current group key" value.
4. On each row:
   - If the row's group key equals the current key → update accumulator.
   - If different → finalize the current group as a row in the output batch, start a new accumulator with the new key.
5. When the child returns `None`, emit the last group, return the output batch.

No hash table. No HashMap allocation. State is one slot per aggregate, not one slot *per group*. Output is naturally sorted.

### Operator: `SortedAggregateExec`

```
struct SortedAggregateExec {
    group_by:     Vec<PhysicalExpr>,      // bare Column refs, matched against sort key
    aggregates:   Vec<AggSpec>,
    child:        Box<dyn ExecutionPlan>,
    output_schema: Arc<Schema>,
    // ... runtime state for the running group + accumulators
}
```

- Stateful, two-phase like HashAggregateExec: pump child to completion, emit one final batch.
- A *streaming-streaming* variant (emit as groups close, not all at end) is a future enhancement — not in this task.

### The planner rule

A new logical-plan optimizer rule or planner pass:

- **Input:** `LogicalPlan::Aggregate { group_by, aggregates, input }` where `input` traces back to a `Scan` of a table with a known `sort_key`.
- **Decision:** If `group_by_columns` is a (possibly empty) **prefix of the sort key**, mark this Aggregate for sorted execution.
- **Output:** Either annotate the plan node, or lower into a distinct `PhysicalPlan` variant (`PhysicalPlan::SortedAggregate { .. }` vs `PhysicalPlan::Aggregate { .. }`). Decision below.

### Sort-key prefix check

Given:
- `sort_key: Vec<usize>` — indices into `TableSchema::columns`
- `group_by: Vec<LogicalExpr::Column(table, col)>` — names

The check:
1. Resolve each `group_by` name to a column index in the schema.
2. Verify the group_by indices form a **set-equal prefix** of `sort_key` — i.e., they cover `sort_key[0..k]` for some `k ≤ group_by.len()`, in any order. (Sort order within sort_key prefix doesn't matter; only that the columns are a prefix.)
3. If yes → sorted path. If no → hash path.

Edge cases:
- Empty `group_by` → trivially the empty prefix → sorted path is valid (degenerates to "one group covering all rows"). But hash path is also fine; either works.
- Single-table-no-sort-key (sort_key is empty) → hash always.

### Multi-part ordering assumption

Streaming aggregation requires the **stream entering the operator to be globally sorted** on the group-by columns. tinyOLAP's storage produces this naturally **iff parts have non-overlapping sort-key ranges and are emitted in part-id order**. For an append-only event log (the smoke case), this holds.

**Decision for this task:** Trust the assumption. Document it. The planner's sorted-path eligibility check assumes append-only ordered parts. Future work could add a precondition check on part metadata (min/max per part) and fall back to hash if violated.

---

## Out of Scope

- **Multi-way merge across parts** when parts overlap. We assume append-only events with monotonic sort-key. If this assumption breaks, the operator either silently produces wrong groups (current proposal — accept the risk) or detects and errors (small future hardening).
- **Streaming-streaming output** — emitting groups as they close, before the child finishes. Possible but adds backpressure complexity. Future.
- **Multi-column sort keys with reordered GROUP BY** — e.g., sort key `[ts, uid]`, `GROUP BY uid, ts`. The current rule treats the GROUP BY column set as unordered, so this *is* eligible for sorted (the output just may emerge in a different column order than the user wrote — fix at projection time).
- **Approximate aggregation, distinct, having** — same as TASK-003.

---

## Decisions Made

- **Two physical operators, not one with a strategy enum.** `HashAggregateExec` and `SortedAggregateExec` are sibling implementations of the same logical operation. Strategy choice lives in the planner, not the operator. This matches ClickHouse and makes the dispatching logic visible.
- **Two `PhysicalPlan` variants:** add `PhysicalPlan::SortedAggregate { .. }` alongside the existing `PhysicalPlan::Aggregate { .. }`. The lowerer from logical → physical inspects sort-key alignment and emits one or the other. Less magic than an annotation on a single variant.
- **Reuse the `Accumulator` trait from TASK-003.** Same trait, same per-aggregate impls. The only difference between hash and sorted is *how groups are identified* (hash table vs running key compare). Accumulators are oblivious.
- **Sort-key alignment check lives in `physical_plan::lower`,** not in a separate optimizer rule. The lowerer already has access to the schema and the logical Aggregate node; threading "is this sort-aligned?" into the lowering is one extra parameter and avoids inventing a "decoration" pass.

---

## Subtask Order

1. **Add `PhysicalPlan::SortedAggregate` variant** to `physical_plan/physical_operators.rs`. Same fields as `Aggregate`. Update `children` / `with_new_children` / Display.
2. **Sort-key prefix check helper** — pure function `is_sort_key_prefix(group_by, sort_key, schema) -> bool`. Unit-testable.
3. **Update `physical_plan::lower::lower`** — when lowering a `LogicalPlan::Aggregate`, run the prefix check, emit `SortedAggregate` if yes, `Aggregate` if no. Threads `schema` into the lowerer (already needed).
4. **`SortedAggregateExec` operator.** Reuses the `Accumulator` trait from TASK-003. The control flow is the running-group-key loop, not a hash table.
5. **Wire into `builder.rs`** — new arm for `PhysicalPlan::SortedAggregate`.
6. **Smoke-test in REPL** — query that should go through sorted (`GROUP BY ts FROM events`), verify result matches hash equivalent. A second smoke query for the hash path (`GROUP BY tag FROM events`) confirms dispatch.
7. **Logging / verification helper** — for debugging the dispatch, optionally print "using SortedAggregate" / "using HashAggregate" with a `--explain` mode flag or similar. Not required, but useful.
8. **Display impl** for the new variant + operator.
9. **Benchmark** — same query under both paths (force-enable hash even when sorted is eligible via a config flag, for comparison). Expect sorted to outperform hash by 3–10× on a few million rows depending on cardinality.

---

## Success Criteria

- `SortedAggregateExec` produces **bitwise-identical output** to `HashAggregateExec` for any query where both are valid (sort-key-aligned GROUP BY).
- The planner picks Sorted automatically for `GROUP BY <prefix of sort_key>` and Hash otherwise.
- A microbenchmark shows the sort-aligned path is meaningfully faster (target: 3× or better on a 1M-row, 100-group aggregation).
- `cargo test` green including a test that asserts dispatch chose the right path.
- The streaming aggregation uses **constant memory** w.r.t. group count — verify with a debug counter or `dhat`.

---

## Key Files Touched

- `src/physical_plan/physical_operators.rs` — new `SortedAggregate` variant + Display/children/with_new_children
- `src/physical_plan/lower.rs` — emit Sorted vs Hash based on sort-key check; new helper `is_sort_key_prefix`
- `src/execution/sorted_aggregate.rs` — new operator
- `src/execution/builder.rs` — new arm
- `src/execution/mod.rs` — module registration
- `tests/sorted_aggregation.rs` (or wherever) — dispatch + correctness tests

---

## Open Questions to Settle Before Coding

- **Annotate or fork the PhysicalPlan variant?** Decided above: fork (`SortedAggregate` as a sibling variant). But re-check if the duplication in `children` / `with_new_children` is irritating; we can refactor toward a single `Aggregate { strategy }` variant later if so.
- **Empty GROUP BY (no group-by columns) — sorted or hash?** Both work identically (one group). Pick by convention: sorted, because it's cheaper. The lowerer dispatches sorted when sort-key check passes; empty group_by passes trivially.
- **Cross-part ordering verification.** Today we assume parts arrive sort-key-monotonic with non-overlapping ranges. Should the operator verify this from `part.zonemap` and fall back to hash if violated? Worth it eventually. Not in this task.
- **Detection of "GROUP BY is permuted prefix of sort key" — order-sensitive or set-sensitive?** Set-sensitive (any permutation of a prefix qualifies). Output column order is controlled by Project anyway.

---

## What This Does NOT Solve

- **Cross-part merge** when parts overlap on sort key — we'd need k-way merge between parts, which is a real engineering item. Future task.
- **Parallel aggregation** across multiple threads. Both Hash and Sorted are single-threaded sequential pulls today.
- **Adaptive switching at runtime** based on observed group count (start sorted, switch to hash if assumption broken mid-stream). DataFusion does this; we don't need it yet.

---

## References

- ClickHouse `src/Processors/Transforms/AggregatingInOrderTransform.cpp` — the streaming aggregator. The names "Aggregating in order" / "Aggregating" map exactly to our Sorted / Hash split.
- DataFusion `physical-plan/src/aggregates/order/` — DataFusion's sort-aware aggregation. They handle it as a property of one operator, not a separate operator; worth reading for contrast with the ClickHouse model.
