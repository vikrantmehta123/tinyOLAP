# tinyOLAP — Benchmark Results

**Date:** 2026-05-18  
**Machine:** Intel Core i5-10210U @ 1.60GHz (4 cores / 8 threads, max 4.2 GHz), Linux 6.8.0  
**Build:** `cargo bench` (release profile, LTO default)  
**Data size:** 1 000 000 rows unless noted

---

## Storage Write — `cargo bench --bench storage_write`

Schema variants:
- **single_i64** — `ts: i64` (1 column, sort key)
- **wide_numeric_5col** — `ts: i64, uid: u32, val: f64, flags: u8, score: f32`
- **mixed_4col** — `ts: i64, uid: u32, event: Str, val: f64`

| Benchmark | Median time | Throughput |
|---|---|---|
| single_i64_1M | 16.3 ms | 469 MiB/s |
| wide_numeric_5col_1M | 82.7 ms | 288 MiB/s |
| mixed_4col_1M | 215.6 ms | 114 MiB/s |

**Takeaways:**
- Wide numeric is ~1.6× slower per raw MB than single column — sort permutation touches all 5 column vectors simultaneously, thrashing cache.
- Mixed (strings + numeric) is another ~2.5× slower on top — string encoding has no fixed stride and can't vectorize.

---

## Storage Scan — `cargo bench --bench storage_scan`

| Benchmark | Median time | Throughput |
|---|---|---|
| single_col_pruned_1M | 21.1 ms | 361 MiB/s |
| all_cols_5col_1M | 66.4 ms | 358 MiB/s |
| string_col_1M | 194.9 ms | 28.4 MiB/s |
| multipart_i64 / 10 × 100k rows | 5.2 ms | 1.43 GiB/s |
| multipart_i64 / 100 × 10k rows | 5.2 ms | 1.43 GiB/s |

**Takeaways:**
- Single-col and 5-col scan throughput is now nearly identical (361 vs 358 MiB/s) — both are LZ4-decompression bound, not I/O bound. Column count matters less than decompression cost.
- String decode remains 12× slower than numeric — bottleneck is 1M heap `String` allocations in the decode loop.
- Parallelism is at the part level (Rayon). Multi-part throughput (1.43 GiB/s) is ~4× single-part — good core saturation. No granule-level parallelism yet.

---

## Encoding Codecs — `cargo bench --bench encoding_codecs`

All benchmarks: 1M `i64` values (8 MB), pure in-memory (no disk I/O).

### Plain

| Benchmark | Median time | Throughput |
|---|---|---|
| encode_i64_1M | 715 µs | 10.4 GiB/s |
| decode_i64_1M | 715 µs | 10.4 GiB/s |

Near-memcpy speed. Ceiling for all other codecs.

### Delta

| Benchmark | Median time | Throughput |
|---|---|---|
| encode_i64_1M / sorted | 3.38 ms | 2.21 GiB/s |
| decode_i64_1M / sorted | 2.90 ms | 2.57 GiB/s |
| encode_i64_1M / random | 3.33 ms | 2.23 GiB/s |
| decode_i64_1M / random | 3.57 ms | 2.09 GiB/s |

Delta is ~5× slower than plain. Sorted and random patterns are nearly identical — arithmetic cost is the same regardless of data pattern.

### RLE

| Benchmark | Median time | Throughput |
|---|---|---|
| encode_i64_1M / high_run | 4.76 ms | 1.57 GiB/s |
| decode_i64_1M / high_run | 3.40 ms | 2.19 GiB/s |
| encode_i64_1M / low_cardinality | 7.67 ms | 994 MiB/s |
| decode_i64_1M / low_cardinality | 5.89 ms | 1.27 GiB/s |
| encode_i64_1M / all_unique | 7.17 ms | 1.04 GiB/s |
| decode_i64_1M / all_unique | 6.17 ms | 1.21 GiB/s |

- `high_run` (all same value) — inner loop just increments a counter, branch-predictor friendly.
- `low_cardinality` (cycling 0–9) — every run has length 1, RLE expands the data. Slower than all_unique.
- RLE only pays off when runs are long (booleans, status flags, low-cardinality enums).

### String Codecs

| Benchmark | Median time | Throughput |
|---|---|---|
| encode_1M / plain | 7.5 ms | 734 MiB/s |
| decode_1M / plain | 106.6 ms | 51.9 MiB/s |
| encode_1M / dictionary | 72.1 ms | 76.7 MiB/s |
| decode_1M / dictionary | 102.5 ms | 54.0 MiB/s |

- Plain encode is 14× faster than plain decode — encode iterates `&[String]` with no allocation; decode creates 1M `String` objects on the heap.
- Dictionary decode ≈ plain decode (~53 MiB/s). Both are heap-allocation bound; codec choice is irrelevant until string materialisation is addressed.
- Fix: `LowCardinality(String)` (Phase 2) — store integer indices internally, allocate only on final output.

---

## Query Pipeline — `cargo bench --bench query_pipeline`

Schema: `ts: i64, uid: u32, event: Str, val: f64`. Single part, 1M rows.

| Benchmark | Median time | Throughput |
|---|---|---|
| full_scan_no_filter | 29.1 ms | 34.4 Melem/s |
| filter_selectivity / 1% | 27.8 ms | 36.0 Melem/s |
| filter_selectivity / 10% | 27.1 ms | 36.9 Melem/s |
| filter_selectivity / 50% | 36.7 ms | 27.3 Melem/s |
| aggregate_no_group_by | 15.1 ms | 66.2 Melem/s |
| group_by / low 10 groups | 380.6 ms | 2.6 Melem/s |
| group_by / high 100k groups | 392.4 ms | 2.5 Melem/s |

**Takeaways:**

- **Filter selectivity is still flat** — without a granule-level index, all rows are read regardless of selectivity. Savings come only from reduced output materialisation.

- **Aggregate beats full scan** — `SUM(val), COUNT(*), AVG(val)` reads only the `val` column (1 of 4). 15ms vs 29ms is the columnar projection win.

- **GROUP BY is 25× slower than aggregate** — ~380ms vs 15ms. Both low (10 groups, string key) and high (100k groups, u32 key) land in the same range. Low-cardinality pays for string hashing over 1M rows; high-cardinality pays for cache misses across a 100k-entry map. Per-row cloning in `group_by_aggregate.rs` is the primary suspect — needs profiling.

---

## What These Numbers Point To

| Area | Current bottleneck | Future fix |
|---|---|---|
| String decode | 1M heap allocations | `LowCardinality(String)` (Phase 2) |
| Numeric scan | Sequential, LZ4-bound, no SIMD | `std::simd` vectorisation (Phase 1) |
| GROUP BY | Per-row string cloning suspected | Profile + fix `group_by_aggregate.rs` |
| Single-part scan | Sequential column reads within part | Granule-level parallelism (Phase 1) |
