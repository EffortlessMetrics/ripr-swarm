# Fixture: rust_const_expectation_literal_control

Spec: RIPR-SPEC-0035

## Given

`quota(2)` is asserted equal to a module `const EXPECTED: u32 = 4`, an independent literal (#7260).

## When

`ripr check` analyzes the changed return value (`n + 1` -> `n + 2`).

## Then

The finding stays `exposed`: the expectation does not depend on the owner.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_lib.rs:return_value:543b6d50 -> exposed
probe:src_lib.rs:static_unknown:788c0b42 -> static_unknown
```

## Must Not

Demote a literal constant expectation because it is spelled as a constant.
