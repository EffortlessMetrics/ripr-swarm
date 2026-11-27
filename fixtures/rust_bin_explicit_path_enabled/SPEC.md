# Fixture: rust_bin_explicit_path_enabled

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`autobins = false` with an explicit `[[bin]] path = "src/main.rs"` declares a
real binary target whose `#[cfg(test)]` unit tests run under `cargo test`.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding promotes to `exposed`.

## Must Not

- Withhold unit tests from an explicitly declared `[[bin]]` target.
