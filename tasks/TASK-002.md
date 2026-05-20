# TASK-002 — Zone Maps: Read Path (Part-Level Pruning)

## Description
Load `part.zonemap` at scan time and use it to skip entire parts whose min/max bounds make the predicate provably false. Pruning is exposed as a new physical operator `ZoneMapScan` that the planner picks when a filterable predicate exists. This turns the zone maps written in TASK-001 from "data on disk" into a real scan optimizer.

---

## Steps

### 1. Parse the zone map file (`src/storage/zone_map.rs`)

- [ ] Add an in-memory type `ZoneMap` — e.g. `HashMap<String, ColumnZone>` where `ColumnZone { type_tag: u8, min_bytes: [u8; 8], max_bytes: [u8; 8] }`
- [ ] Add `fn read_zone_map(part_dir: &Path) -> io::Result<ZoneMap>`
  - Parses the header (version, col_count, per-column entries)
  - For part-level, `entry_count = 1` per column — one min/max pair each
  - Uses the per-column `offset` from the header to seek to the data section
- [ ] Validate `version == 1`; return an error on mismatch

### 2. Pruning logic (`src/storage/zone_map.rs`)

- [ ] Implement `fn can_skip(zone: &ZoneMap, predicate: &Predicate) -> bool`
- [ ] `Cmp { col, op, value }`:
  - If `col` is not in the zone map (string column or excluded), return `false` (do not prune)
  - Widen `value` to match the column's `type_tag`, then apply:
    - `Eq` → skip if `value < min || value > max`
    - `Lt` → skip if `min >= value`
    - `Le` → skip if `min > value`
    - `Gt` → skip if `max <= value`
    - `Ge` → skip if `max < value`
    - `Ne` → never skip
- [ ] `And(a, b)` → skip if `can_skip(a) || can_skip(b)`
- [ ] `Or(a, b)` → skip only if `can_skip(a) && can_skip(b)`
- [ ] `Not(_)` → return `false` (conservative; defer proper handling)

### 3. New operator `src/processors/zone_map_scan.rs`

- [ ] Implements `Processor`
- [ ] Holds: `Predicate`, the list of part ids, an internal cursor
- [ ] For each part: load `part.zonemap`, call `can_skip(predicate)`; if true → skip; otherwise → produce the part's batch
- [ ] Share part-reading logic with `FullScan` — factor a small helper (`read_part(part_id) -> Batch`) that both call, or compose `ZoneMapScan` around a part-iterator abstraction
- [ ] Expose a `parts_skipped` counter (eprintln for now) for verification

### 4. Planner rewrite

- [ ] When the plan is `Filter(FullScan)`, rewrite to `Filter(ZoneMapScan(predicate))`
- [ ] `Filter` stays in place — zone maps prune coarsely at the part level; `Filter` still does row-level filtering
- [ ] When there is no `WHERE`, the plan keeps using `FullScan` unchanged

### 5. Test

- [ ] Insert two parts with non-overlapping `ts` ranges (e.g. part A: ts=1..100, part B: ts=200..300)
- [ ] Run `SELECT * FROM events WHERE ts > 150`
- [ ] Assert correct rows come back AND `parts_skipped == 1`

---

## Out of Scope
- Granule-level pruning (deferred — `entry_count` already supports it in the file format)
- String column zone maps
- Predicate pushdown for `Not` and complex `Or` beyond the conservative rules above
- Bloom filters
- Sharing more than just part-reading between `FullScan` and `ZoneMapScan`
