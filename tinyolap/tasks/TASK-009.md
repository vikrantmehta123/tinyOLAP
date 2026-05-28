# TASK-009 — Background Compaction: Merge Algorithm

## Description
Implement the k-way merge that combines N sorted parts into one larger sorted part. Covers the merge logic only — the scheduler and reader integration are TASK-010. Prerequisite: TASK-008 design decisions must be filled in.

---

## Steps

### Module scaffold (`src/compaction/`)

- [ ] `mod.rs` — re-exports
- [ ] `merger.rs` — merge algorithm (this task)
- [ ] `scheduler.rs` — TASK-010

### k-way merge (`src/compaction/merger.rs`)

- [ ] Accept `parts: Vec<u32>` (part ids to merge) and `table_dir: &Path`
- [ ] Open a `ColumnReader` (or `StringColumnReader`) per column per input part
- [ ] Use a min-heap keyed on the primary-key column value to select the next row
- [ ] Accumulate rows into output `ColumnChunk` buffers; flush to a new part via `TableWriter` when buffers reach a threshold
- [ ] Output lands in `tmp_part_NNNNN/`; rename to `part_NNNNN/` on success
- [ ] On any failure: delete `tmp_part_NNNNN/`, leave source parts untouched

### Atomicity

- [ ] Source parts are deleted only after the rename succeeds
- [ ] If the process crashes mid-merge, `tmp_part_*` directories are orphans — add cleanup of `tmp_*` dirs on startup in `main.rs`

### Test

- [ ] Write 3 parts with interleaved but within-part-sorted rows
- [ ] Call the merge function directly (no scheduler)
- [ ] Assert: output part contains all rows in globally sorted order
- [ ] Assert: source parts are deleted; merged part is readable via `TableReader`

---

## Out of Scope
- Scheduler (TASK-010)
- Reader integration / part visibility (TASK-010)
- External merge sort for parts exceeding memory (Phase 2)
