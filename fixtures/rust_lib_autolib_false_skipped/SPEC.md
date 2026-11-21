# Fixture: rust_lib_autolib_false_skipped

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`autolib = false` with no `[lib]` entry means `src/lib.rs` is not a Cargo
target, so its `#[cfg(test)]` unit tests never run under `cargo test`.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding stays `no_static_path` and must not promote.

## Must Not

- Credit unit tests in an undiscovered `src/lib.rs` under `autolib = false`.
