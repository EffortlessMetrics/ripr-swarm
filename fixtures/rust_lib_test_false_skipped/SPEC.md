# Fixture: rust_lib_test_false_skipped

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`[lib] test = false` disables the library unit-test target in `cargo test`, so
the `#[cfg(test)]` module in `src/lib.rs` is never compiled or executed.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding stays `no_static_path` and must not promote.

## Must Not

- Credit unit tests in a `[lib] test = false` target.
