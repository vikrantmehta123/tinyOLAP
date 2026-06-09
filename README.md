# tinyOLAP

A small columnar database, written in Rust.

It is a work-in-progress and it is not trying to be a production ready project.

## What Works Today

- Data can be INSERTed into a table. Data lives on disk as immutable parts, lz4-compressed, with per-column files and a mark index for granule lookup.
- SELECT statements with filters and projections. GROUP BY statements with aggregations (COUNT, SUM, MIN, MAX, AVG). 
- Parallelized Query Processing: a query pipeline is cloned and executed in parallel, and a gather operation collects results from parallel threads.
- Zone Maps over numeric columns. Zone Maps are implemented at a granularity of a part. If skip_predicate doesn't pass, then reading the entire part will be skipped.

## What Doesn't Work

- No DELETE, UPDATE statements
- No compaction of parts.
- No indexes.
- The SQL surface is narrow. The parser is `sqlparser-rs`; the analyzer that lowers it is hand-rolled and will reject most things outside the path above.
- No network layer, no client protocol. Only CLI interface is present.

## What's Next

1. Compaction of parts in a background task.
2. LIMIT clause support
3. Threadpool for concurrent query execution

## Benchmarks

A criterion-driven suite of 8 SELECT queries over a fixed 25M-row, 13-column synthetic `events` table (~1 GB on disk, lz4-compressed, 5 parts). Dataset and seed are fixed (`SmallRng::seed_from_u64(42)`).

Results below are from a 4-thread run measured against a serial baseline, on a laptop.

## Results

Latest run (`granule_level_read` baseline), 25M rows. Time is criterion's
median estimate; throughput is rows scanned per second.

| #  | Query                                                                                   | Time     | Throughput      |
|----|-----------------------------------------------------------------------------------------|----------|-----------------|
| Q1 | `SELECT SUM(price) FROM events`                                                         | 54.5 ms  | 458.3 Melem/s   |
| Q2 | `SELECT SUM(price), AVG(quantity), MIN(duration_ms), MAX(duration_ms), COUNT(price) FROM events` | 301.7 ms | 82.9 Melem/s    |
| Q3 | `SELECT SUM(price) FROM events WHERE ts < 250000`                                        | 89.4 ms  | 279.6 Melem/s   |
| Q4 | `SELECT SUM(price) FROM events WHERE country_id < 100`                                   | 158.6 ms | 157.6 Melem/s   |
| Q5 | `SELECT country_id, SUM(price) FROM events GROUP BY country_id`                          | 351.6 ms | 71.1 Melem/s    |
| Q6 | `SELECT user_id, SUM(price), COUNT(price) FROM events GROUP BY user_id`                  | 3.438 s  | 7.27 Melem/s    |
| Q7 | `SELECT country_id, city_id, AVG(price) FROM events WHERE ts > 5000000 GROUP BY country_id, city_id` | 6.160 s  | 4.06 Melem/s    |
| Q8 | `SELECT event_type, SUM(price) FROM events GROUP BY event_type`                          | 2.818 s  | 8.87 Melem/s    |

See [`tinyolap/benches/README.md`](tinyolap/benches/README.md) for the full methodology — dataset rationale, query selection, criterion config.

## Design Decisions

- Marks are at granule level and not row level. Row level indexes will grow metadata. Granule-level index keeps the index small enough to hold in memory and can be efficiently used in indexes.
- ZoneMaps are also kept at a part level.
- Arrow format for in-memory buffers. On-disk columnar shape matches arrow format and this provides SIMD computation on arrays.

## On-Disk Layout of Data

One INSERT produces one part. Parts are immutable. A write goes to `tmp_part_NNNNN/` and is atomically renamed into place on success, so a crash mid-insert never leaves a half-written part visible to readers.

Inside a column file, data is grouped into **granules** of 512 values. A granule is the atomic addressable unit — one mark per granule, and predicate pushdown skips at part granularity. Multiple granules pack into a **block** of roughly 8 KiB uncompressed bytes per lz4 call.

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
