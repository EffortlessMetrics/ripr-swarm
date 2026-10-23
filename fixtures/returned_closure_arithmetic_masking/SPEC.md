# Fixture: returned closure arithmetic masking

Spec: RIPR-SPEC-0265 rule 5 and RIPR-SPEC-0108.

Exact assertions run the returned closures but do not discriminate the changed
operator at the supplied values: a captured offset, unit input, an identifier
spelled like a literal, and an uppercase helper call. Shared owner/closure names cannot substitute for an applied discriminating pin.
All six return-value
findings must remain present and must not be promoted to `exposed`. Public API
controls live in `returned_closure_pin.rs`; native original/mutated fixtures
and successful direct/qualified/captured-value controls are retained in the
review proof.
