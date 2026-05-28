# Repository Layout

This repo has two independent areas:

## `tinyolap/`
The main project — a columnar database in Rust. This is the serious, perf-first work. Has its own `CLAUDE.md` with detailed design decisions, task management, and collaboration rules. **Read `tinyolap/CLAUDE.md` before doing any work in that directory.**

## `examples/`
A sandbox for standalone learning experiments (SIMD playgrounds, assembly inspection, small Rust/other-language explorations). Each subdirectory is an independent Cargo project with its own `Cargo.toml`, `Cargo.lock`, and `target/`. There is **no workspace** — examples are built individually and have no dependency relationship to `tinyolap/`.

Examples are throwaway by nature. Collaboration style for any given example is decided per-example.

## Build
- `tinyolap/`: `cd tinyolap && cargo build`
- An example: `cd examples/<name> && cargo build`

There is no top-level `cargo` command — nothing at the repo root is a Cargo project.
