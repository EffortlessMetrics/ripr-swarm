# Fixture: typescript_call_removal_multiple_calls

Spec: RIPR-SPEC-0243

Counted removal receiver witness (#10157).

## Given

Two original receiver calls are replaced by two distinct discards. The test
installs the correct spy before the owner and asserts one call. Native Node
rejects both original (two calls) and removal (zero calls) at that assertion.

## When

The CLI analyzes both removed statements in `diff.patch`.

## Then

Exactly two candidate-current findings remain `weakly_exposed`. The complete
file diff cannot establish the bounded single-line replacement witness.
The independent promotion-honesty corpus pins both findings and refuses credit.

## Must Not

- Treat per-line counterparts as proof of exactly one old invocation.
- Infer discrimination from a green unrelated corpus or a count of one alone.
