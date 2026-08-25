# RIPR-SPEC-0166: Rust subprocess binary reach limitation

Status: proposed

## Purpose

Name the boundary where an integration test invokes a Cargo-built binary but
RIPR cannot yet map that executable back to the changed Rust owner.

## Contract

For a `no_static_path` finding whose owner is in `src/main.rs` or `src/bin/**`,
RIPR may emit:

```text
static_limit_kind = rust_subprocess_binary_reach_unresolved
```

when an integration test contains either a `Command::new` invocation using
`CARGO_BIN_EXE_*` with `.output()`/`.status()`, or an `assert_cmd`
`cargo_bin(...)` invocation with an assertion/output/status observation.

The recognizer is source-only and deterministic. It does not spawn a process,
parse arbitrary shell commands, infer a binary target map, or credit the test
as reaching any owner.

## Safety

Classification remains `no_static_path`. The limitation must not create a
related test, promote exposure, claim receipt validity, or emit a repair
packet. Direct unit-test and existing transitive/macro limitation behavior is
unchanged.

## Follow-up boundary

Counting a subprocess test as evidence requires a separate Cargo binary-target
map and a bounded owner relation. That work is intentionally not part of this
limitation slice.
