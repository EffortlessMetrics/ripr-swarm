# Fixture: predicate_oracle_gated_mod_ungated_owner_refused

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #14537

## Given

A `#[cfg(feature = "lang-python")]` module cannot have its build availability
established statically. The changed owner is an ungated `lib.rs` item, so the
crate compiles it whether or not the feature is on, while the test that checks
it sits inside the gated module and reaches the owner through `crate::gate`.

## When

`cargo xtask fixtures predicate_oracle_gated_mod_ungated_owner_refused` analyzes
the diff.

## Then

The predicate equality is refused at the module-declaration step: the owner is
not reached only through the gated declaration, so the unknown gate can
separate the test from the owner and grants no credit. The finding records
`observe` and `discriminate` as `no` with `rust_assertion_context_unestablished`
and prints the `Not credited:` line naming `mod python;` at `src/lib.rs:2`. The
class is `propagation_unknown` because propagation is unknown for this shape.

```expected-classes
probe:src_lib.rs:predicate:574ee60a -> propagation_unknown
```

## Must Not

- Admit the equality because the gate's availability is unknown.
- Treat the unknown feature gate as definitely enabled or definitely disabled.
- Claim runtime execution or full boundary-pairing coverage.
