# Fixture: predicate_oracle_gated_mod_admitted

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #14537

## Given

A `#[cfg(feature = "lang-python")]` module cannot have its build availability
established statically. The changed owner lives inside that module, so the
crate reaches it only through the gated `mod python;` declaration, and its test
directly checks the changed boundary with an ordinary `assert_eq!`.

## When

`cargo xtask fixtures predicate_oracle_gated_mod_admitted` analyzes the diff.

## Then

Exactly one predicate finding reads `propagation_unknown`. The exact-value
oracle is admitted under the #14463 shared-gate rule: the owner is compiled
exactly when the test is, so the unknown gate cannot separate them, and the
finding records `observe` and `discriminate` as `yes`. Propagation stays
`unknown` because the owner lives in a module whose build availability ripr
cannot establish, so it cannot follow the returned value statically.

```expected-classes
probe:src_python.rs:predicate:037c6af2 -> propagation_unknown
```

## Must Not

- Treat the unknown feature gate as definitely enabled or definitely disabled.
- Admit an owner the crate also compiles without the gated declaration (see
  `predicate_oracle_gated_mod_ungated_owner_refused`).
- Admit a provably-disabled module, an include-only context, or a real macro shadow.
- Claim runtime execution or full boundary-pairing coverage.
