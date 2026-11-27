# Fixture: rust_operator_fn_shadows_constructor

Spec: RIPR-SPEC-0001

Owner: analysis-fixtures

Issue: #15277

## Given

`impl Add for Money { fn add }` changes from `self.0 - other.0` to
`self.0 + other.0`, and `Cash` has its own `Add`. The test module declares
`fn Money(v: i32) -> Cash`, so `Money(1) + Money(2)` in the test adds two
`Cash` values. The test asserts `Money(1) + Money(2) == Cash(3)`. It
passes on the old code and on the new code, because it never runs
`Money`'s impl.

## When

`cargo xtask fixtures rust_operator_fn_shadows_constructor` runs the public diff analyzer.

## Then

The `return_value` finding does not read `exposed`. The shared type-scope
check of the operator, equality, iterator, drop, parse and `.into()`
readings refuses when a value item named like the impl type shadows its
constructor.

## Must Not

- Credit `impl Add for Money` because each operand is spelled
  `Money(..)`.
