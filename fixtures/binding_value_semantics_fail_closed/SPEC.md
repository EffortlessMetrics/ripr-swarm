# Fixture: binding_value_semantics_fail_closed

Spec: RIPR-SPEC-0158

## Given

Three boundaries that host-index folding cannot establish. For `signed`,
`0_i32.checked_sub(1)` returns `Some(-1)`, so both `< 0` and `<= 0` return
true; the authored test does not land on the changed equality boundary.
For `eager`, `find` returns `Some(1)` but Rust still evaluates the default
`1 / n`, which panics for the authored argument zero. The latter is an
intentional adversarial static-analysis fixture, not a passing Rust test.
For `narrow::score`, the name `usize` is an alias for `u8`. At 200, the
direct expression `(n + 100) / 2` panics with overflow checks enabled and
evaluates to 22 with them disabled; neither behavior reaches boundary 150.
Treating the spelling as the intrinsic `usize` would fabricate that equality.

## When

```bash
cargo xtask fixtures binding_value_semantics_fail_closed
```

## Then

All three predicate findings remain `infection_unknown`. The local operands
retain their unresolved `k`; the shadowed direct expression cannot invent an
exact equality or name a missing test input. Public API tests separately retain genuine `usize` arithmetic,
intrinsic string indices, bounded wide-integer arithmetic, and safe computed
and literal `map_or` defaults.

The separate returned-comparison probes retain the inherited `exposed`
fallback (three rows). This fixture and the honesty-corpus pin constrain the
predicate family; they do not establish that fallback's correctness.

## Must Not

- Treat the signed checked subtraction as unsigned underflow to default zero.
- Skip an eager default because the option contains a value.
- Promote any predicate from an invented equality observation.
- Claim that this fixture's Rust tests were executed successfully.
