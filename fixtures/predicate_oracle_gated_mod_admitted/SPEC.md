# Fixture: predicate_oracle_gated_mod_admitted

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #14537

## Given

A `#[cfg(feature = "lang-python")]` module cannot have its build availability
established statically. Its test directly checks the changed boundary with an
ordinary `assert_eq!`, and no later source-provenance refusal exists.

## When

`cargo xtask fixtures predicate_oracle_gated_mod_admitted` analyzes the diff.

## Then

Exactly one predicate finding reads `weakly_exposed`. The exact-value oracle is
admitted because the gated module's availability is unknown, not because the
module is known to run. The far assertion can provide weak evidence without
pairing the boundary.

## Must Not

- Treat the unknown feature gate as definitely enabled or definitely disabled.
- Admit a provably-disabled module, an include-only context, or a real macro shadow.
- Claim runtime execution or full boundary-pairing coverage.
