# Fixture: rust_lib_autolib_false_skipped

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`autolib = false` with no `[lib]` entry means `src/lib.rs` is not a Cargo
target. Target discovery (#15892) drops it, so the changed file is seen but
yields no probe.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The report counts one changed Rust file and zero findings. Nothing promotes.
The analysis outcome is incomplete and names the unreached file as a limitation,
so the zero is not reported as a clean result.

## Must Not

- Credit unit tests in an undiscovered `src/lib.rs` under `autolib = false`.
- Report a finding for a file that is not a Cargo target.
