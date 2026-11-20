# Fixture: rust_stub_todo_expected_value

Spec: RIPR-SPEC-0231

Owner: analysis-fixtures

Issue: #14810

## Given

The changed predicate uses `amount >= discount_threshold`. The two existing
tests execute the non-boundary inputs 50/100 and 10_000/100, so neither
discriminates the boundary. A third test is the `ripr agent stub --write`
template: it binds `let expected: i32 = todo!("ripr: write the value ...")`
and asserts `assert_eq!(discounted_total(amount, threshold), expected)` with
the placeholder still in place.

## When

`cargo xtask fixtures rust_stub_todo_expected_value` runs the public diff
analyzer.

## Then

The one predicate finding reads `weakly_exposed`, not `exposed`: an assertion
compared with a local bound only to an unfilled placeholder macro pins no
expected value, so it is not a discriminator. The assertion stage reads
`unknown` (not established) rather than `yes`.

## Must Not

- Credit the stub's `todo!()` expected value as a strong exact-value oracle.
- Downgrade a filled expected value (`let expected: i32 = 9_990;`) or a real
  expression expected value: both keep their exact credit.
- Downgrade an assertion that never reads the placeholder-bound local.
- Treat a non-placeholder local as unfilled, or read a `todo!()` in a comment
  or string as a binding.
