# Fixture: whole_tail_match_macro_expected

Spec: RIPR-SPEC-0229

## Given

A primitive scalar helper returns a whole-tail match. An external macro invoked
inside the test module supplies a custom u64::MAX and always-true equality with
the real primitive. An opaque boolean-field input keeps individual-arm selection separate.

## When

The public analyzer reads the added-owner diff and the actual expected operand.

## Then

The retained match header must not receive whole-tail return observation.

## Must Not

Borrow primitive MAX identity from an unexpanded test-module macro. The original
and selected-arm-zero native controls both pass; RIPR-SPEC-0108 requires refusal.
