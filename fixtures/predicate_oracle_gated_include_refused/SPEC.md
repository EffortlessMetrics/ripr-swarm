# Fixture: predicate_oracle_gated_include_refused

Spec: RIPR-SPEC-0197

Owner: analysis-fixtures

Issue: #14537

## Given

A `#[cfg(feature = "lang-python")]` module composes the crate with `mod python;`,
and `src/python.rs` only includes `src/frag.rs`. The included test uses the
changed boundary in an ordinary equality.

## When

`cargo xtask fixtures predicate_oracle_gated_include_refused` analyzes the diff.

## Then

Exactly one predicate finding reads `static_unknown`. The gated module exception
skips only its own Module edge; the remaining Include edge refuses the test with
the exact `include!("frag.rs")` note.

## Must Not

- Attribute strong exact-value oracle evidence to the include fragment.
- Skip the Include edge or its provenance after admitting the gated Module edge.
- Confuse this refusal with the positive gated-module control.
