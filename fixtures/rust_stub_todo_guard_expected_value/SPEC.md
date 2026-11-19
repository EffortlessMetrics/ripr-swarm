# Fixture: rust_stub_todo_guard_expected_value

Spec: RIPR-SPEC-0231

Owner: analysis-fixtures

Issue: #14882

## Given

The changed return value is `amount - 10`. Two tests call `price(100)`
with the expected value still an unfilled `todo!()`. One checks it with a
terminal guard, `if price(100) != expected { return Err(..) }`. The other
uses a weak equality, `assert!(price(100) == expected)`.

## When

`cargo xtask fixtures rust_stub_todo_guard_expected_value` runs the public
diff analyzer.

## Then

The return-value finding does not read `exposed`. Neither the guard's
assertion twin nor the weak equality pins a value while the expected local
is bound only to a placeholder. The owner return pin therefore has nothing
to raise to strong.

## Must Not

- Credit the terminal guard or the weak equality on a `todo!()` expected
  value as a discriminator.
- Downgrade the same guard on a literal value (`if price(100) != 90`). The
  integration test `terminal_guard_unfilled_expected.rs` keeps that control.
