# Fixture: whole_tail_match_scrutinee_return

Spec: RIPR-SPEC-0229

This refusal control also exercises RIPR-SPEC-0108.

## Given

A newly added documented scalar helper has a statement-free tail match whose
scrutinee returns 600 before the match selects an arm. A direct equality pins
only that bypass value. The input helper keeps individual arm tokens out of the
assertion.

## When

`cargo xtask fixtures whole_tail_match_scrutinee_return`

## Then

The match header and both individual arms must not be exposed. The return value
of the scrutinee is a different observation from the match result.

## Must Not

Construct an unconditional whole-tail match return receipt from this body or
credit branch coverage from an equality that all arm mutations leave passing.
The original, Some=>0, None=>0 and both=>0 native controls compile and all pass
because the scrutinee bypasses every match result.
