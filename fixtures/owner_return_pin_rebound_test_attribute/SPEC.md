# Fixture: owner_return_pin_rebound_test_attribute

Spec: RIPR-SPEC-0197

## Given

`weight(input)` changes from `input * 2` to `input * 3`. The test module
imports another attribute macro under the name `test`
(`use skipper::serial as test;`), so `#[test]` is no longer libtest's
attribute and the macro may replace the body (#6749). With a body-removing
macro, `cargo test` runs zero tests and a wrong `weight` survives.

## When

`cargo xtask fixtures owner_return_pin_rebound_test_attribute` analyzes the
fixture. `input/skipper` is a real
proc-macro crate whose `serial` attribute drops the annotated function:
`cargo test` in `input/` compiles and reports `running 0 tests`, so a wrong
`weight` would survive. The analyzer must not credit the assertion.

## Then

One return-value finding is not promoted: the assertion earns no owner-return
pin, so the finding stays `static_unknown` with the test attribute named as
the unresolved context.

## Must Not

- Promote to `exposed` while the test attribute may be another macro.
- Claim a gap: the replacement macro may well run the body.
