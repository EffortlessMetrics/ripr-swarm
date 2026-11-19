# Fixture: rust_bin_autobins_false_skipped

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`autobins = false` with no `[[bin]]` entry means `src/main.rs` is not a Cargo
target, so its `#[cfg(test)]` unit tests never run under `cargo test`.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding stays `no_static_path` and must not promote.

## Must Not

- Credit unit tests in an undiscovered `src/main.rs` under `autobins = false`.
