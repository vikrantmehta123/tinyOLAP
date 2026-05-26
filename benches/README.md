# tinyOLAP query benchmarks

Single criterion-driven SELECT benchmark suite. INSERT performance is
intentionally out of scope — what matters for tinyOLAP is how fast we can
**scan, filter, and aggregate** the parts that already exist on disk.

## Running

```bash
# First run generates the dataset under benches/data/events/ (~1 GB, ~1 min).
# Subsequent runs reuse it.
cargo bench --bench queries

# Save a named baseline (do this before any optimization).
cargo bench --bench queries -- --save-baseline serial

# After an optimization, compare against the baseline. Criterion prints
# %change per query with statistical significance.
cargo bench --bench queries -- --baseline serial

# List benches without running.
cargo bench --bench queries -- --list

# Run a single query.
cargo bench --bench queries -- Q6_groupby_high_cardinality_int

# HTML reports are written to target/criterion/.
```

Delete `benches/data/events/` to force regeneration (e.g. after changing the
dataset spec).

## Why this exists

Every performance change we make — parallelism, SortedAggregate, ZoneMaps,
compaction, morsels — is a *claim*. Without a measurement harness, those
claims are unverifiable and untellable. This bench locks in a baseline so
every future optimization has a number it has to beat, automatically tracked
by criterion's `--save-baseline` / `--baseline` workflow.

The suite is deliberately small (8 queries, ~3–4 min per full run). The
queries are picked to **cover distinct code paths under conditions that
distinguish good implementations from bad ones**, not to mimic a real
workload.

## Dataset

A single denormalized `events` table — ClickBench-style philosophy on a
miniature scale.

- **Rows:** 25,000,000
- **Parts:** 5 (5M rows each), sorted by `ts` end-to-end
- **On-disk size:** ~1 GB compressed (lz4)
- **Generator seed:** fixed (`SmallRng::seed_from_u64(42)`) — every machine
  sees the same dataset.

### Schema (13 columns)

| Column        | Type   | Cardinality / range          | Role           |
|---------------|--------|------------------------------|----------------|
| `ts`          | i64    | monotonic, jittered          | sort key       |
| `user_id`     | i64    | ~1M distinct                 | high-card key  |
| `country_id`  | i16    | 200 distinct                 | low-card key   |
| `city_id`     | i32    | 10,000 distinct              | mid-card key   |
| `device`      | i8     | 5 distinct                   | very low-card  |
| `event_type`  | String | 20 distinct                  | string key     |
| `price`       | f64    | 0 – 1000                     | agg target     |
| `quantity`    | i32    | 1 – 50                       | agg target     |
| `duration_ms` | i32    | 0 – 600,000                  | agg target     |
| `flag_a`      | bool   | 50/50                        | padding        |
| `flag_b`      | bool   | 50/50                        | padding        |
| `session_id`  | i64    | ~uniform                     | padding        |
| `event_code`  | i16    | ~uniform                     | padding        |

The 4 **padding columns** are never queried. They exist to widen rows so
that "single-column scan vs all-columns scan" is a measurable difference —
the whole point of columnar storage.

### Why these sizes

- **25M rows.** Big enough that CPU differences are visible (Q1 ~500 ms, Q6
  ~2 s on a typical laptop), small enough that the full suite finishes in a
  few minutes and the dataset fits comfortably in RAM during gen.
- **5 parts of 5M.** Multiple parts so future parallel-scan work has
  something to fan out across; not so many that part-discovery becomes the
  bottleneck before compaction lands.
- **`user_id` ~1M distinct.** Stresses high-cardinality GROUP BY but keeps
  the hash table at ~50–70 MB — well within RAM. tinyOLAP **does not spill
  to disk**, so the working set must fit in memory by construction.
- **String column at 20 distinct values.** Pairs with `country_id` (200
  int groups) so the Q8/Q5 delta isolates *string hashing cost*, not
  group-count effects.

## The 8 queries

Each query exercises a distinct axis. None of them are arbitrary — every
one exists to make a specific perf signal visible.

| #  | Bench id                          | Tests                                                                |
|----|-----------------------------------|----------------------------------------------------------------------|
| Q1 | `Q1_scan_sum_single_column`       | Pure scan + SUM on one column. Best case for columnar. Baseline.     |
| Q2 | `Q2_scan_many_aggregates`         | 5 accumulators on one scan — amortizes per-batch overhead.           |
| Q3 | `Q3_filter_low_selectivity`       | ~0.5% rows pass. Future ZoneMap baseline (granule skipping).         |
| Q4 | `Q4_filter_high_selectivity`      | ~50% rows pass. Filter cost without filter benefit.                  |
| Q5 | `Q5_groupby_low_cardinality_int`  | ~200 int groups. Hash table fits L2. HashAggregate happy path.       |
| Q6 | `Q6_groupby_high_cardinality_int` | ~1M int groups. Hash table blows cache. Future SortedAggregate win.  |
| Q7 | `Q7_filter_plus_multikey_groupby` | Filter + 2-key GROUP BY + AVG. Operator-chain overhead.              |
| Q8 | `Q8_groupby_string`               | ~20 string groups. Q8 − Q5 ≈ string-hashing overhead.                |

### Query SQL

```sql
-- Q1
SELECT SUM(price) FROM events;

-- Q2
SELECT SUM(price), AVG(quantity), MIN(duration_ms), MAX(duration_ms), COUNT(price)
FROM events;

-- Q3 (sort-aligned filter; ts ∈ [0, ~50M), so <250000 is ~0.5%)
-- TYPE-COERCION WORKAROUND — see note below. Intent: country_id = 5.
SELECT SUM(price) FROM events WHERE ts < 250000;

-- Q4 (~50% of rows; user_id is random, not sort-aligned)
-- TYPE-COERCION WORKAROUND — see note below. Intent: country_id < 100.
SELECT SUM(price) FROM events WHERE user_id < 500000;

-- Q5 (~200 groups)
SELECT country_id, SUM(price) FROM events GROUP BY country_id;

-- Q6 (~1M groups)
SELECT user_id, SUM(price), COUNT(price) FROM events GROUP BY user_id;

-- Q7 (filter ~80% rows, then 2-key GROUP BY)
SELECT country_id, city_id, AVG(price)
FROM events WHERE ts > 5000000 GROUP BY country_id, city_id;

-- Q8 (~20 string groups)
SELECT event_type, SUM(price) FROM events GROUP BY event_type;
```

## Methodology

- **Criterion config.** `sample_size = 20`, `warm_up_time = 2s`,
  `measurement_time = 25s` (generous so slow queries like Q6 still fit
  20 samples in the budget). Default `sample_size = 100` is geared at sub-ms
  microbenchmarks; for 200ms–2s queries it makes the suite multiple
  minutes longer with no statistical benefit (run-to-run variance is low
  because the dataset is fixed and warm).
- **Throughput.** Annotated as `Elements(25_000_000)` — criterion prints
  `rows/s` alongside ms. Note this is *rows scanned*, not *rows produced*;
  for filtered queries (Q3, Q4, Q7) it's still 25M because there's no
  granule-skipping today. After ZoneMaps land, Q3's "rows scanned" will
  drop and the throughput number gets more nuanced — revisit then.
- **Setup cost excluded.** Dataset gen happens once before the bench group
  starts. Table-schema load happens once. Only the query (parse → plan →
  execute → materialize batches) is inside `b.iter`.
- **`black_box` on result.** Otherwise LLVM may elide the whole query.
- **Warm cache only.** Criterion's warm-up phase runs the query a few
  times without recording, which also warms the OS page cache. We are
  CPU-bound today (parallelism, then SortedAggregate are the next wins);
  cold-cache mode adds platform complexity for little signal until I/O
  becomes the bottleneck.

## Known workaround: integer-literal type coercion

Q3 and Q4 were originally designed to filter on `country_id` (i16, 200
distinct values) — `country_id = 5` for low selectivity and
`country_id < 100` for high. They were rewritten to use `ts` and
`user_id` (both i64) because of an engine gap:

> Integer literals always lower to i64 (`LiteralValue::Int(i64)`), and
> the filter operator (`src/execution/expr.rs`) builds the literal as an
> `Int64Array` Scalar regardless of the column's actual type. Arrow's
> comparison kernels require both sides to share a DataType, so
> `country_id (Int16) = 5 (Int64)` is rejected with
> "Invalid comparison operation: Int16 == Int64".

The proper fix is a **TypeCoercion** rule in the logical optimizer that
walks `Compare(Column(c), Literal(...))`, looks up `c`'s type, and
inserts a `Cast` node around the literal. Tracked as a follow-up task
(separate from this bench work). When it lands, Q3 should revert to
`WHERE country_id = 5` and Q4 to `WHERE country_id < 100` — the
semantics (~0.5% / ~50% selectivity) are preserved by the current i64
substitutes but the original intent is closer to a real OLAP workload
(filtering on small-int dimension columns).

## What this bench is NOT

- **Not a TPC-H / ClickBench equivalent.** Far fewer queries, smaller
  schema, no joins. tinyOLAP doesn't support joins yet — when it does,
  add a `joins.rs` bench file alongside this one rather than expanding
  this suite.
- **Not an INSERT benchmark.** tinyOLAP's performance story is about
  reads on durable parts. INSERT-heavy workloads belong in a separate
  suite if we ever care.
- **Not a regression gate.** Nothing here runs in CI by default —
  criterion's baseline-diff is a developer workflow, not a pass/fail check.
- **Not a memory benchmark.** Times only. If memory becomes a concern,
  pair this with `valgrind --tool=massif` or `heaptrack` separately.

## The baseline workflow

```bash
# Once, before any optimization work:
cargo bench --bench queries -- --save-baseline serial

# After every optimization, e.g. parallelism:
cargo bench --bench queries -- --save-baseline parallel
cargo bench --bench queries -- --baseline serial
# Criterion prints per-query %change vs the "serial" baseline.

# Multiple stored baselines coexist — diff any pair:
cargo bench --bench queries -- --baseline parallel
```

This is the actual asset the bench produces: a per-optimization speedup
table that's reproducible from a fixed seed on any machine.
