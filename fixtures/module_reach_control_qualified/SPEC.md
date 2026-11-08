# Fixture: module_reach_control_qualified

Spec: RIPR-SPEC-0159

## Given

A module-qualified own-library call `qual_reach::inner::order_discount(..)` in an integration test. The changed line is the private helper `is_bulk` in `src/inner.rs`.

## When

```bash
cargo xtask fixtures module_reach_control_qualified
```

## Then

Related to the changed helper through the entry (control for the refusals).

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_inner.rs:predicate:25dc8b63 -> exposed
probe:src_inner.rs:return_value:438cae72 -> weakly_exposed
```

## Must Not

- Promote the finding on a counterexample, or lose the relation on the controls.
