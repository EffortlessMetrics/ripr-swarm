# Fixture: rust_example_test_true_enabled

Spec: RIPR-SPEC-0356

Owner: analysis-fixtures

Issue: #15012

## Given

`[[example]] name = "demo" test = true` enables unit tests in `examples/demo.rs`
under `cargo test`.

## When

`cargo xtask goldens check` runs on the fixture diff.

## Then

The return-value finding promotes to `exposed`.

## Must Not

- Withhold unit tests from an example target with `test = true`.
