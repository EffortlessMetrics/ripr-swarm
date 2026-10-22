# Fixture: field_match_unexecuted_assertion

Spec: RIPR-SPEC-0229

## Given

A direct record call is compared with an exact expected value inside `if false`.
The parsed constructor would select the changed arm, but the assertion never
executes. The unchanged runtime test also passes an incorrect arm return.

## When

`cargo xtask fixtures field_match_unexecuted_assertion`

## Then

The changed arm subjects must not be exposed. The direct-record extension
must pass the existing assertion execution admission before its same-call
pairing can supply discrimination. A direct executed assertion still qualifies.

## Must Not

Treat a parsed call in an unreachable assertion as executed oracle evidence,
or duplicate the existing parser-backed assertion execution authority.
