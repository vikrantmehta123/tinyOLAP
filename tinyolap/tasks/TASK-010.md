# TASK-010 — Background Compaction: Scheduler and Reader Integration

## Description
Wire the merger from TASK-009 into a background thread that watches part count and triggers merges automatically. Update `FullScan` / `TableReader` to use a shared, lock-guarded part list so readers never see a mid-merge part.

**Prerequisite: TASK-009.**

---

## Steps

### Shared part list

- [ ] Wrap the part list in `Arc<RwLock<Vec<u32>>>` (part ids)
- [ ] `FullScan` acquires a read lock at construction, clones the vec, releases immediately — queries are never blocked by the lock itself
- [ ] `TableWriter` acquires a write lock only to append the new part id after a successful INSERT

### Part tombstoning

- [ ] When the scheduler selects a merge set, mark those part ids as `being_merged` in the shared list (e.g. `Arc<RwLock<HashSet<u32>>>` for the tombstone set)
- [ ] `FullScan` skips tombstoned parts when cloning its working set

### Scheduler (`src/compaction/scheduler.rs`)

- [ ] Spawn a `std::thread` that polls the part list on a short timer
- [ ] Trigger condition: part count > `MERGE_THRESHOLD` (add to `config.rs`)
- [ ] Selection: pick the smallest-N parts by total `.bin` file size
- [ ] Send a `MergeJob { parts: Vec<u32> }` over a `std::sync::mpsc::channel` to a worker thread

### Worker thread

- [ ] Receives `MergeJob`, calls `merger::merge`
- [ ] On success: write lock → remove tombstoned ids, add merged part id
- [ ] On failure: write lock → clear tombstones, log the error

### Graceful shutdown

- [ ] On REPL exit (Ctrl-D): send shutdown signal to scheduler thread; `join()` before process exit

### End-to-end test

- [ ] Insert 12 parts via 12 separate INSERTs
- [ ] Wait for the scheduler to trigger (or call it directly in test)
- [ ] Verify part count dropped; `SELECT *` returns all rows in sorted order

---

## Out of Scope
- Multiple concurrent merge workers
- Merge throttling
- External merge sort (Phase 2)
