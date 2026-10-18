# Fixture: whole_tail_match_macro_scalar

Spec: RIPR-SPEC-0229

## Given

An external module defines an exported macro that introduces a struct named u64
with PartialEq<i32> that always succeeds. A newly added whole-tail match returns
that custom type; an assertion compares the result to a literal. An opaque input
helper keeps individual-arm selection separate from the header-only regression.

## When

The public analyzer checks the complete added-owner diff.

## Then

The retained match header must not receive whole-tail return-observation credit.
The original and selected/other/both-arm-zero mutants all compile and pass.

## Must Not

Infer primitive identity from a return spelling supplied by an unexpanded
module-level macro. RIPR-SPEC-0108 retains this false-promotion refusal.
