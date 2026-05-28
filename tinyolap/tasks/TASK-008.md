# TASK-008 — Background Compaction: Design Review

## Description
Before writing any compaction code, spend one session sketching the merge algorithm and concurrency model. This is the MergeTree mechanic that makes the project credible for DB-infrastructure roles. The output of this task is a filled-in design section below — no code.

---

## Steps

- [ ] **Phase decision**: is this Phase 1 or Phase 2?
  - Phase 1: adds the most impressive talking point but also the most risk
  - Phase 2: safe, but weakens the story vs ClickHouse-style systems
  - Record decision here

- [ ] **Sketch the merge algorithm**
  - k-way merge of N sorted parts on the primary key
  - Memory model: read one granule at a time from each input part (bounded memory)
  - Output: written via `TableWriter` to `tmp_part_NNNNN/`, renamed atomically on success

- [ ] **Sketch the concurrency model**
  - How does a reader know which parts are stable vs being merged?
  - Options: `Arc<RwLock<Vec<PartHandle>>>`, generation counter, tombstones in the part list
  - How does a concurrent insert avoid interfering with an in-progress merge?

- [ ] **Sketch the scheduler**
  - Trigger: part count > threshold (e.g. 10) OR total small-part size > threshold
  - Selection: smallest-N-parts-first (ClickHouse style)
  - Threading: dedicated `std::thread` vs `tokio` task

---

## Design Decisions (fill in during the session)

**Phase decision:**

**Merge algorithm:**

**Concurrency model:**

**Scheduler:**

---

## References
- `src/storage/table_writer.rs` — the write path the merger will reuse
- ClickHouse `MergeTreeDataMergerMutator`: `/Personal/open-source/ClickHouse/src/Storages/MergeTree/`
