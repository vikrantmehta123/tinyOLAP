# tinyOLAP

A small columnar database, written in Rust.

It is a work-in-progress and it is not trying to be a production ready project.

## What Works Today

- Data can be INSERTed into a table. Data lives on disk as immutable parts, lz4-compressed, with per-column files and a mark index for granule lookup.
- SELECT statements with filters and projections. GROUP BY statements with aggregations (COUNT, SUM, MIN, MAX, AVG). 
- Parallelized Query Processing: a query pipeline is cloned and executed in parallel, and a gather operation collects results from parallel threads.

## What Doesn't Work

- No DELETE, UPDATE statements
- No compaction of parts.
- No indexes, or zone-maps.
- The SQL surface is narrow. The parser is `sqlparser-rs`; the analyzer that lowers it is hand-rolled and will reject most things outside the path above.
- No network layer, no client protocol. Only CLI interface is present.

## What's Next

1. Zone Maps Per Granule
2. SIMD Friendly Hash-Map Lookup
3. Compaction of parts in a background task.

## Benchmarks

A criterion-driven suite of 8 SELECT queries over a fixed 25M-row, 13-column synthetic `events` table (~1 GB on disk, lz4-compressed, 5 parts). Dataset and seed are fixed (`SmallRng::seed_from_u64(42)`).

Results below are from a 4-thread run measured against a serial baseline, on a typical laptop.

| Query | What it tests                          | Serial   | 4 threads | Speedup |
|-------|----------------------------------------|----------|-----------|---------|
| Q1    | Scan + SUM, single column              | ~250 ms  | 185 ms    | 1.35×   |
| Q2    | Scan + 5 aggregates                    | ~870 ms  | 563 ms    | 1.54×   |
| Q3    | Low-selectivity filter (~0.5% pass)    | ~440 ms  | 430 ms    | ~1.0×   |
| Q4    | High-selectivity filter (~50% pass)    | ~495 ms  | 280 ms    | 1.77×   |
| Q5    | GROUP BY low-card int (~200 groups)    | ~1.95 s  | 1.18 s    | 1.66×   |
| Q6    | GROUP BY high-card int (~1M groups)    | ~12.1 s  | 7.10 s    | 1.71×   |
| Q7    | Filter + 2-key GROUP BY                | ~12.9 s  | 12.05 s   | 1.07×   |
| Q8    | GROUP BY string (~20 groups)           | ~5.75 s  | 4.20 s    | 1.37×   |

A few notes:

- **Speedups cap well below 4×.** Operator-chain overhead, per-batch dispatch, and gather contention all bite. Q7's 1.07× is the clearest sign — the filter + multi-key GROUP BY chain spends a lot of time outside the parallelizable scan.
- **Q3 barely moves** because today every query scans every granule. Zone maps (next on the roadmap) should turn Q3 into a near-no-op rather than a parallelism problem.
- **Running parallel code paths on 1 thread is ~50% slower than the original serial baseline.** The parallel dispatch overhead is real and only pays off above 1 thread.

See [`tinyolap/benches/README.md`](tinyolap/benches/README.md) for the full methodology — dataset rationale, query selection, criterion config, and why throughput numbers will need re-interpretation once granule-skipping lands.

## Design Decisions

- Marks are at granule level and not row level. Row level indexes will grow metadata. Granule-level index keeps the index small enough to hold in memory and can be efficiently used in indexes.
- Arrow format for in-memory buffers. On-disk columnar shape matches arrow format and this provides SIMD computation on arrays.

## On-Disk Layout of Data

One INSERT produces one part. Parts are immutable. A write goes to `tmp_part_NNNNN/` and is atomically renamed into place on success, so a crash mid-insert never leaves a half-written part visible to readers.

Inside a column file, data is grouped into **granules** of 512 values. A granule is the atomic addressable unit — one mark per granule, and predicate pushdown skips at granule granularity. Multiple granules pack into a **block** of roughly 8 KiB uncompressed bytes per lz4 call.

This design is inspired by ClickHouse.

```bash
table_root/
├── part_00001/
│   ├── user_id.bin     ← compressed column data
│   ├── user_id.mrk     ← granule index: offsets + row counts
│   ├── country.bin
│   └── country.mrk
├── part_00002/
│   └── ...
```

### Supported Types
`i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64`, `f32`, `f64`, `bool`, variable-length strings

## Try It

tinyOLAP reads a `schema.json` from the table directory at startup. Create one before running:

```bash
mkdir -p data/my_table
cat > data/my_table/schema.json << 'EOF'
{
  "name": "my_table",
  "columns": [
    { "name": "ts",    "data_type": "I64" },
    { "name": "uid",   "data_type": "U32" },
    { "name": "value", "data_type": "F64" },
    { "name": "tag",   "data_type": "Str" }
  ],
  "sort_key": [0]
}
EOF
```

`sort_key` is a list of column indices (zero-based) that form the primary key. The default table directory is `data/tinyolap_smoke`. To use a different directory, edit `tinyolap/src/config.rs`.


```bash
git clone https://github.com/vikrantmehta123/tinyOLAP
cd tinyOLAP/tinyolap
cargo run --release
```

```sql
tinyOLAP ready. Table: 'my_table'
Type SQL and press Enter. Ctrl-D to quit.

> INSERT INTO my_table VALUES (1700000000, 1, 9.5, 'cpu'), (1700000060, 2, 3.1, 'mem');
OK (2 rows inserted, part_0)

> SELECT ts, tag FROM my_table WHERE uid = 1;
...

> SELECT tag, SUM(value) FROM my_table GROUP BY tag;
...
```

## Repo Layout

```
tinyolap/   the database
examples/   unrelated scratchpad — SIMD experiments, assembly inspection, etc.
```