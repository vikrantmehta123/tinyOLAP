# Rust (and more) from Scratch

A scratchpad for understanding how things work underneath the abstractions. Experiments here might implement SIMD in assembly, build concurrency primitives from scratch, explore a threadpool, or dissect a data structure like SwissTable.

The goal is not to ship anything — it is to understand things deeply enough that using them in [tinyolap/](../tinyolap/) is never a black box.

## Structure

Each subdirectory is a standalone experiment. It may be a Cargo project, a C file, assembly, or just notes and code snippets. There is no workspace and no shared dependencies.

## Experiments

| Directory | What it explores |
|---|---|
| `concurrency_playground/` | Concurrency primitives from scratch in Rust |
