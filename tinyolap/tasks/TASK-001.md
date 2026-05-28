# TASK-001

## Title
Parallelize Query Execution with Scatter/Gather and a Worker Pool

## Description

Today, query execution in tinyOLAP is vectorized but single-threaded: a plan
runs batch-by-batch on one thread, from `Scan` through `Filter`, `Project`,
`Aggregate`, etc. The goal of this task is to parallelize execution by running
multiple copies of the stateless segment of the pipeline concurrently across a
pool of long-lived worker threads, and merging their outputs at a single
`Gather` point.

The dataflow between operators will be **push-based**: each worker drives its
own pipeline segment forward and pushes finished batches into a bounded channel
that the `Gather` operator drains. Workers pull *work assignments* (not data)
from a shared queue exposed by the scan side.

### Design constraints

- **Unit of parallelism is swappable.** The scan side will expose a
  `ScanWorkSource` trait whose `next_work()` returns an opaque chunk of work.
  For this task, the unit is one **part** per assignment. The interface must be
  shaped so that switching to per-granule-range later is a localized change
  (new enum variant + new source implementation + scan operator learning to
  read a range), not a pipeline-wide refactor.
- **Push-based dataflow** between operators, using `crossbeam_channel` with
  bounded channels. Workers pull assignments from the shared work source but
  push batches downstream.
- **Worker count is a config knob** defaulting to `num_cpus::get()`.
- **Gather is non-order-preserving** for now. Batches arrive in whatever order
  workers finish. Order-preserving merge is explicitly out of scope.
- **No NUMA, no morsel-driven execution.** These remain out of scope.
- **Parallel aggregation IS in scope.** Cloning the plan per worker means
  every worker runs its own `HashAggregateExec` independently, which produces
  N partial results that must be merged for correctness. Aggregation cannot
  be left serial without losing the speedup the whole task aims for, and
  cannot be left as N partials without producing wrong answers. The merge
  step is therefore required, not deferred.
- **No speculative abstractions.** Don't introduce machinery for features that
  aren't being built in this task. Refactor when the next task demands it.

### Scope of this task

In scope:
- Introduce a `ScanWorkSource` trait and a per-part implementation.
- Introduce a `Gather` physical operator that owns N receiver channels and
  emits batches as they arrive.
- Stand up a long-lived worker pool that each run a full copy of the stateless
  pipeline segment (`Scan -> Filter -> Project -> ...`) and push results into
  `Gather`.
- Plumb a `parallel_degree` setting through the planner with the configured
  default.
- Partial-then-merge parallel aggregation. Each worker maintains its own
  `HashAggregateExec` instance. A new `AggregateMergeExec` operator combines
  the N partial hash tables into the final result. `Accumulator` gains a
  `merge` method (Sum: add, Count: add, Min/Max: per-group, Avg: combine
  sum + count).
- Demonstrate end-to-end parallel execution with measurable speedup vs. the
  serial baseline on the full benchmark suite, with results matching the
  serial path on every query (including aggregation).

Out of scope (deferred):
- Per-granule-range work units.
- Order-preserving gather / merge.
- Hash-repartitioned aggregation (true partitioned aggregate; this task
  uses the simpler partial-then-merge model).
- Spilling for partials that don't fit in memory.
- Parallel merge of partials (the merge step itself is single-threaded
  in this task).
- Backpressure tuning, cancellation, error-propagation polish beyond a
  working baseline.
- Threshold-based fallback to serial execution for small scans.

## Expected Outcomes

1. Every query (scan-only, filter, aggregation, group-by) executes across N
   worker threads and produces results identical to the serial path (modulo
   row order).
2. `ScanWorkSource` is the single point that determines the unit of work;
   changing the unit later does not require touching `Gather`, the worker
   loop, or operators above `Scan`.
3. `Gather` is present at the root of every parallel plan and merges N streams
   into one without preserving order.
4. Aggregation queries are correct under N>1. Each worker computes a partial
   hash table; `AggregateMergeExec` combines them into the final result.
5. Benchmarks show measurable speedup vs. the serial baseline across the full
   benchmark suite. The exact target multiplier isn't fixed up front — we
   measure, then decide what's acceptable.
6. Worker count is controllable via config, defaulting to `num_cpus::get()`,
   and can be set to 1 to recover deterministic serial behavior for tests.

## Status

- ScanWorkSource trait + per-part implementation: done.
- Gather operator (N inputs, one shared MPMC channel, deadlock-safe Drop): done.
- Worker pool (N=4 hardcoded, long-lived workers each owning a cloned plan): done.
- Builder fan-out (clone PhysicalPlan N times, share Arc<ScanWorkSource>): done.
- Non-aggregation queries: correct results, measurable speedup (1.05x-1.88x).
- Aggregation queries: parallelized but INCORRECT — N partial hash tables are
  concatenated rather than merged. Bug to fix.

## Next Step

Add `merge` to the `Accumulator` trait. Each accumulator type
(Sum / Count / Min / Max / Avg) defines how two partial states combine.
Then design the `AggregateMergeExec` operator that consumes N partial result
batches from `Gather` and emits the merged final result.

