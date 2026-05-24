# TASK-005 — Zero-copy String Column Materialization

## Context

This task captures a known performance debt introduced during TASK-002 (Arrow adoption). It is **not blocking** any other task. Take it up when:

- A workload with string-heavy columns is being benchmarked.
- Profiling shows allocator pressure during reads of string columns.
- The string-column read path is the next bottleneck.

---

## The Problem

`StringColumnReader::read_all` currently produces `Arc::new(StringArray::from(Vec<String>))`. The internal flow is:

```
disk bytes
    │  (lz4 decompress)
    ▼
decompressed bytes
    │  (StringCodec::decode writes into Vec<String>)
    ▼
Vec<String>                  ← allocates one String per element
    │  (StringArray::from)
    ▼
StringArray (offsets + flat payload)   ← Arrow re-copies all bytes into one buffer
    │
    ▼
ArrayRef
```

For an 8 KiB block holding ~800 strings (avg 10 bytes each):
- **800 String allocations** at decode time.
- **800 String drops** after Arrow copies their bytes.
- **One redundant memcpy** of every byte (decoded → Vec<String> heap → Arrow payload).

For a 1 M-row column: ~1.25 M wasted allocator round-trips per scan.

The cost is invisible until profiled, but it is real and grows linearly with row count. For a portfolio-grade engine, this is exactly the kind of perf hole that recruiters flag.

---

## The Fix

Arrow's `StringArray` is `(offsets: Buffer<i32>, payload: Buffer<u8>)`. To build it without per-element allocation, the codec needs to write **directly into Arrow-shaped buffers** — either:

### Option A — Codec writes to a `StringBuilder`

`StringBuilder` wraps the offsets and payload internally; `append_value(&str)` extends both. The codec's `decode` signature becomes:

```rust
fn decode(&self, bytes: &[u8], out: &mut StringBuilder) -> Result<(), DecodeError>;
```

The reader instantiates a builder once, calls `decode` for each block, then `builder.finish()` returns the `StringArray`. **One allocation per block** (the builder's internal buffers reuse capacity across `append_value` calls).

### Option B — Codec emits raw `(payload, offsets)`

The codec writes into a `&mut Vec<u8>` payload and a `&mut Vec<i32>` offsets, both reused across blocks. After all blocks decoded:

```rust
let array = StringArray::new(
    OffsetBuffer::new(ScalarBuffer::from(offsets)),
    Buffer::from(payload),
    None, // validity
);
```

Even fewer allocations, more direct, but requires understanding Arrow's `OffsetBuffer` / `ScalarBuffer` plumbing.

**Recommendation:** Option A — `StringBuilder` is idiomatic Arrow and easier to read. Option B is faster by a small margin but harder to maintain.

---

## Files Touched

- `src/encoding/string_codec.rs` (or wherever `StringCodec` lives) — change `decode` signature.
- `src/storage/string_column_reader.rs` — use `StringBuilder`, finish at the end of `read_all`.
- `src/storage/string_column_writer.rs` — consumer side may need adjustment (depends on how writer is built; likely the writer is unaffected because it operates on the input side, not the codec output).
- Any other caller of `StringCodec::decode` — grep first to confirm.

---

## Success Criteria

- `read_all` returns `ArrayRef` (already true after TASK-002) but with **zero per-element String allocations** during decode.
- Microbenchmark: scan a 1 M-row string column. Allocations measured via `dhat` or `jemalloc::stats`. Should drop from ~1.25 M to single-digit thousand.
- No regression in existing tests.
- Same on-disk format — this is purely a read-path optimization.

---

## What This Does NOT Cover

- Writer-side perf (the writer encodes incoming Arrow arrays into the on-disk format — separate path).
- Dictionary encoding for low-cardinality strings (`DictionaryArray`) — future work, much bigger task.
- Compression scheme changes — orthogonal.

---

## Estimated Effort

Small. One day of focused work. The codec interface change is mechanical; the reader change is ~10 lines; the test surface is small (existing tests verify correctness, microbenchmark for perf).
