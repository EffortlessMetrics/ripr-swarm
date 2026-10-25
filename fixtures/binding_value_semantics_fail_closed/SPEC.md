# Fixture: binding_value_semantics_fail_closed

Spec: RIPR-SPEC-0158

## Given

Two local boundaries that host-index folding cannot establish. For `signed`,
`0_i32.checked_sub(1)` returns `Some(-1)`, so both `< 0` and `<= 0` return
true; the authored test does not land on the changed equality boundary.
For `eager`, `find` returns `Some(1)` but Rust still evaluates the default
`1 / n`, which panics for the authored argument zero. The latter is an
intentional adversarial static-analysis fixture, not a passing Rust test.

## When

```bash
cargo xtask fixtures binding_value_semantics_fail_closed
```

## Then

Both predicate findings remain `infection_unknown` and name the unresolved
local operand `k`. Neither can invent an exact equality or name a missing
test input. Public API tests separately retain genuine `usize` arithmetic,
intrinsic string indices, bounded wide-integer arithmetic, and safe computed
and literal `map_or` defaults.

## Must Not

- Treat the signed checked subtraction as unsigned underflow to default zero.
- Skip an eager default because the option contains a value.
- Promote either predicate from an invented equality observation.
- Claim that this fixture's Rust tests were executed successfully.
