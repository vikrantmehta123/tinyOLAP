# TASK-001 — Zone Maps: Write Path (Part-Level)

## Description
During INSERT, compute `(min, max)` per column per part and write a single `part.zonemap` file alongside the column `.bin` and `.mrk` files. String columns are excluded. This is the write path only — the read path (loading zone maps and skipping in FullScan) is TASK-002.

---

## File Format

### `part.zonemap` binary layout

**Header** (read once at open time):
```
[version: u8]
[col_count: u16]
for each column:
  [col_name_len: u16]
  [col_name: utf8 bytes]
  [type_tag: u8]          ← discriminant of ColumnType
  [entry_count: u32]      ← 1 for part-level; N granules when extended later
  [offset: u64]           ← byte offset into this file where this column's entries start
```

**Data** (one block per column, in header order):
```
for each column:
  [min: 8 bytes LE][max: 8 bytes LE]   ← repeated entry_count times
```

**Type encoding** — all min/max values stored as 8 bytes:
- `i8/i16/i32/i64` → sign-extend to `i64`, store as 8-byte LE
- `u8/u16/u32/u64` → zero-extend to `u64`, store as 8-byte LE
- `f32/f64` → widen to `f64`, store as 8-byte LE bits

The header offset lets the reader seek directly to any column's entries without scanning others.

**Extensibility:** `entry_count = 1` today. When granule-level zone maps are added, `entry_count = ceil(total_rows / GRANULE_SIZE)` with one min/max pair per granule in mark order — no format change needed.

---

## Steps

### Compute min/max during write (`src/storage/column_writer.rs`)

- [ ] After writing all values for a column, compute `min` and `max` over the full column (skip `ColumnChunk::Str`)
- [ ] Store the result as `(i64, i64)` or `(u64, u64)` or `(f64, f64)` depending on the column type — decide on a uniform in-memory representation

### Serialize `part.zonemap` (`src/storage/part_writer.rs` or equivalent)

- [ ] After all columns are written, collect each column's `(type_tag, min, max)`
- [ ] Compute the data offset for each column (header is variable-length, so offsets must be calculated after the full header is known)
- [ ] Write the header then the data section as described above
- [ ] Write to `tmp_part_NNNNN/part.zonemap` — the existing atomic rename covers it

### Test

- [ ] Insert a part with known numeric columns and a predictable min/max
- [ ] Read back `part.zonemap` manually (parse the bytes) and assert the correct min/max values are stored for each column

---

## Out of Scope
- String columns (no zone map entry written for them)
- Read path / granule skipping (TASK-002)
- Granule-level zone maps (deferred)
- Bloom filters (deferred)
