# Fixture: rust_example_default_skipped

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

An example under `examples/demo.rs` defaults to `test = false` in Cargo, so its
`#[cfg(test)]` tests are never executed by `cargo test`.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding stays `no_static_path` and must not promote.

## Must Not

- Credit `#[test]` functions inside an example target without `test = true`.
