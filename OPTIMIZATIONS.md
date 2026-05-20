# tinyOLAP — Query Execution Optimizations

Workload assumptions:
- Inserts are bounded — at most a few thousand rows, worst case ~100k, ~1 MB per insert.
- A part is atomic per insert (one `insert()` produces one finalized part).
- Reads are batch scans / aggregations, not point lookups.

---

## Defining Throughput

**Uncompressed bytes of column data processed per second**, end-to-end from query
submission to aggregated result. Concretely: `(total_rows × bytes_per_row) /
query_wall_time`. This accounts for both I/O and CPU work and scales with data size.

The full scan pipeline:
```
Disk → [I/O read] → compressed bytes → [LZ4 decompress] → raw bytes
     → [codec decode] → Vec<T> → [batch assemble] → [aggregate]
```

Each stage is a potential bottleneck.

---

## Layer 1: I/O

### 1.1 `pread` / Positioned I/O

Today's `read_granule` does:
```rust
self.bin.seek(SeekFrom::Start(mark.block_offset))?;
self.bin.read_exact(&mut compressed)?;
```

That's two syscalls and mutates the file's shared current-position cursor.
`FileExt::read_exact_at` (Unix `pread`) is one syscall, takes the offset as an
argument, and does not touch the cursor.

The bigger win: no shared mutable cursor means multiple threads can hold `Arc<File>`
and issue concurrent reads without a `Mutex`. This directly unblocks column-level
parallelism within a part (§2.2 below).

### 1.2 `mmap`

Maps file contents into virtual address space. Reading becomes pointer arithmetic
into a `&[u8]`; the kernel pages blocks in on demand.

- **Zero-copy**: compressed bytes are already a `&[u8]` — skip the read-into-buffer copy.
- **Trivial parallelism**: multiple threads dereference the same region. No locks.
- **`.mrk` files are ideal**: the mark file is a packed array of fixed-size records.
  With `#[repr(C)]` + `bytemuck::Pod`, `cast_slice(&mmap)` gives you `&[Mark]` for
  free — zero parse cost, zero allocation.

**Caveat:** mmap I/O errors become `SIGBUS`, not `Result::Err`. A truncated file
crashes the process instead of returning an error. Read-only mmap is safe; avoid
mmap for writes.

**Recommendation:** mmap the `.mrk` files now (large win, low risk). For `.bin`,
use `pread` first (errors are recoverable), move to mmap later if profiling warrants it.

### 1.3 Async I/O (`io_uring`)

The ceiling for single-machine I/O throughput. Instead of blocking threads on reads,
submit all read requests to the kernel simultaneously and process completions as they
arrive. NVMe SSDs have hardware queues of 64K requests — synchronous reads with 8
threads leave most of that queue empty.

`tokio-uring` or `glommio` expose this on Linux. Architecturally invasive (the whole
pipeline must become async). Defer until the synchronous path is profiled and confirmed
to be I/O-bound after the simpler wins are taken.

---

## Layer 2: CPU (Decompression + Decode)

### 2.1 `target-cpu=native` in Release Builds

lz4_flex has SIMD-accelerated paths for x86_64 that only activate when the compiler
knows the target supports AVX2/SSE4.2. Add to `.cargo/config.toml`:

```toml
[profile.release]
rustflags = ["-C", "target-cpu=native"]
```

Can **double decompression throughput** with zero code changes. Trivial to do.

### 2.2 Column-Level Parallelism Within a Part (TASK-005)

Currently, for a part with 10 columns, those 10 columns are read and decompressed
sequentially inside the rayon closure. Each column's decompression is fully independent.
With `pread` / `Arc<File>` (§1.1), multiple threads can issue concurrent reads on the
same file descriptor, unlocking column-level parallelism without opening multiple file
handles.

For wide tables (20+ columns), this is the next major throughput lever after part-level
parallelism.

### 2.3 Operate on Encoded Data (Pushdown into Codec)

For some aggregations, full decode is unnecessary:
- `COUNT(*)` never needs to decode — just read the mark count from `part.meta`.
- `MIN`/`MAX` on sorted columns = just the first/last granule (or read directly from
  `part.meta` if min/max is stored there).
- `SUM` over delta-encoded sorted integers: the partial sum over the encoded deltas
  avoids decoding every intermediate value.

Very complex to implement generally, but enormous speedup for specific cases. ClickHouse's
`MergeTree` achieves near-constant-time min/max queries this way.

---

## Layer 3: Memory and Cache

### 3.1 Granule-Level Streaming Aggregation

**Current model:** `read_all()` → full `Vec<T>` for the entire column → hand to
aggregator. For a 10M-row `i64` column, that is 80 MB allocated, used once, freed.

**Better model:** process one granule at a time, aggregate immediately, discard:
```rust
for each granule:
    let slice = reader.next_granule()?;  // &[T] borrowed from block cache
    acc += slice.iter().sum::<T>();       // sum without materializing
    // granule bytes freed on next iteration
```

Working set stays in L1 cache (GRANULE_SIZE=512 × 8 bytes = 4 KB per granule).
Eliminates the large per-column allocation entirely. Requires a borrowed-scan reader
API (`fn next_granule(&mut self) -> io::Result<&[T]>`) and either a new processor
path or a `process_granule` method that bypasses `next_batch()`.

### 3.2 Projection Pushdown — Verify It Works

`FullScan` already accepts a `columns: Vec<ColumnDef>` and only reads those columns.
Verify the executor is actually pruning the column list before constructing `FullScan`.
A `SELECT SUM(price) FROM events` that inadvertently reads `user_id` and `name` wastes
I/O proportional to those columns' sizes. Free throughput if the analyser is not already
doing this.

### 3.3 Batch Size Tuning

Currently one part = one batch. Parts with very different row counts give rayon's
work-stealing unequal work units — one thread can do 10× the work of others. Splitting
at granule boundaries (512 rows each) gives much finer-grained load balancing.

---

## Layer 4: Query Planning

### 4.1 Late Materialization

Current model: read all columns → apply filter → aggregate.

Better model for selective queries:
1. Read only the filter column.
2. Compute a bitmask of matching row indices.
3. Use the bitmask to read only matching values from other columns.

For a query matching 1% of rows, you read 1% of the data for non-filter columns.
ClickHouse calls this "late materialization" — it is a primary reason columnar stores
outperform row stores on analytical queries. Implementation requires the filter and
projection processors to cooperate on a shared bitmask rather than operating on full
materialized batches independently.

---

## Supporting Infrastructure

### 5.1 `part.meta`

A part today is "whatever files are in the directory". You cannot ask how many rows it
has, what the min/max of a column is, or whether the part is complete.

A `part.meta` file written **last** (after all `.bin` and `.mrk` are fsynced) and itself
fsynced before the rename solves all of this:
```
part_00042/
  user_id.bin
  user_id.mrk
  price.bin
  price.mrk
  part.meta   ← written and fsynced last
```

Crash recovery: any part directory without a valid `part.meta` is incomplete and gets
deleted on startup. Contents: magic + version, row count, per-column min/max/null count/
encoding/byte size, writer timestamp. Enables crash recovery and richer future metadata.

### 5.2 Per-Block Checksums

A single bit-flip in a `.bin` file causes either silent data corruption or an lz4 panic.
Store an `xxhash3_64` of each compressed block:
```
[u32 compressed_len][u64 hash][compressed bytes]
```
On read, hash and compare. Mismatch → return an error and quarantine the part.

### 5.3 Magic Bytes + Format Version

First 8 bytes of every `.bin` and `.mrk`: `b"TINYOLAP"` + a `u8` format version.
A reader opening a v2 file with v1 code should fail loudly, not parse garbage.

---

## Priority Order

| # | Optimization | Section | Impact | Effort |
|---|---|---|---|---|
| 1 | `target-cpu=native` | §2.1 | Medium | Trivial |
| 2 | Verify projection pushdown | §3.2 | High | Low |
| 3 | `part.meta` + magic/version | §5.1, §5.3 | Crash recovery + future metadata | Medium |
| 4 | Per-block checksum | §5.2 | Correctness | Low |
| 5 | `pread` on `.bin`, mmap on `.mrk` | §1.1, §1.2 | Medium + unblocks col parallelism | Medium |
| 6 | Column-level parallelism within a part | §2.2 | High (wide tables) | Medium |
| 7 | Granule-level streaming aggregation | §3.1 | High (large datasets) | Medium |
| 8 | Late materialization | §4.1 | Very high (selective queries) | High |
| 9 | Async I/O (`io_uring`) | §1.3 | High ceiling | High |
