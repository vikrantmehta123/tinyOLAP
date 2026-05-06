# tinyOLAP — Feature Spec

## Phase 1

### Storage

1. [X] **Abstractions**: `ColumnWriter`, `ColumnReader`, `TableWriter`, `TableReader` with LZ4 compression.
2. [X] **Data types**: integers (`i8`–`u64`), `f32`/`f64`, `bool`, variable-length strings. All non-null columns. 
3. [X] **Schema**: single table, frozen at startup. One column designated as the primary key (sort key).
4. [X] Add encoding schemes like Delta, RLE.

### Query Processing & Executor

1. [X] **WHERE**: predicates (`=`, `<`, `>`, `<=`, `>=`, `!=`, `AND`, `OR`, `NOT`) evaluated column-by-column.
2. [X] **Aggregations**: `SUM`, `AVG`, `COUNT`, `MIN`, `MAX` — with `GROUP BY`
3. [] **Parallelism**: `rayon` across granules and parts and aggregations.
4. [] **SIMD**: vectorized arithmetic and comparisons in the hot path via `std::simd`. Core learning goal — required.
5. [X] **Sorted Parts**: each INSERT batch is written as an immutable "part" — a directory of per-column files, sorted by primary key. 
6. [] **Support for HAVING clause**


### Query Parsing

Two statement forms, no more:
```sql
INSERT INTO defaulttable VALUES (...)
SELECT x, y, agg(z) FROM defaulttable WHERE <cond> GROUP BY <cols> HAVING <cond>
```

---

## Phase 2 (after Phase 1 is end-to-end)

1. **Background merging**: merge multiple sorted parts into one larger sorted part while queries run.
2. **External merge sort**: merge algorithm for parts that exceed memory.
3. **Merge scheduler**: background thread that triggers merges on part count / size thresholds.
4. **Partitioning**: data partitioned by a single column (e.g. a date). Each partition is an independent directory of parts.
5. LowCardinality(String) type. And dictionary encoding for the same.
6. Nullable columns. 

---

## Out of scope

- Replication
- Distributed sharding
- Transactions / MVCC
- Multiple tables
- Schema changes post-creation

---

## Stretch

- `HyperLogLog` for approximate COUNT DISTINCT
- `CountMinSketch` for approximate frequency queries
- Additional codecs: ZSTD, Delta, DoubleDelta
