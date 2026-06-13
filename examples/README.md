# Rust (and more) from Scratch

A scratchpad for understanding how things work underneath the abstractions. Experiments here might implement SIMD in assembly, build concurrency primitives from scratch, explore a threadpool, or dissect a data structure like SwissTable.

The goal is not to ship anything — it is to understand things deeply enough that using them in [tinyolap/](../tinyolap/) is never a black box.

## Structure

Each subdirectory is a standalone experiment. It may be a Cargo project, a C file, assembly, or just notes and code snippets. There is no workspace and no shared dependencies.

## Experiments

| Directory | What it explores |
|---|---|
| `simd_playground/` | How the compiler auto-vectorizes, and SIMD-friendly data structures/algorithms |
| `concurrency_playground/` | Concurrency primitives from scratch in Rust |

### `simd_playground/`

`NOTES.md` covers how `rustc` or LLVM auto-vectorize (loop & SLP vectorizers, inlining). Each binary in `src/bin/` is a self-contained experiment:

- `bad.rs` — a prefix sum with a loop-carried dependence and other bad examples.
- `good.rs` — a SIMD-friendly prefix sum using a shift-and-add algorithm and other good examples.
- `swisstable.rs` — a simpler implementation of a SwissTable. The hash table is assumed to be fixed size and no resizing is there.

### `concurrency_playground/`

Concurrency primitives built from scratch. Each binary in `src/bin/` is a self-contained experiment:

- `atomics.rs` — atomics basics.
- `mutex.rs` — a mutex from scratch.
- `channel.rs` — a minimal single-producer/single-consumer, single-item channel.
- `threadpool.rs` — a threadpool built on the channel.
