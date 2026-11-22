# Fixture: python_warn_keyword_continuation_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`headers` calls `warnings.warn(...)` over several lines and changes only the
`stacklevel=2,` continuation line to `stacklevel=3,`. The only test asserts
`headers("x") == {"a": "x"}`; the returned value does not depend on the
warning, so the assertion passes with either value.

## When

`cargo xtask fixtures python_warn_keyword_continuation_no_exposed_credit`

## Then

The finding is not `exposed`. The keyword line takes the shape of the call it
continues (a side effect), as the same call has on one line, and the changed
integer literal feeds an imported call the adapter does not evaluate, so the
finding reads `weakly_exposed`. This is the #16018 false-exposed correction.

## Must Not

Credit the return-value assertion as observing the warning argument. The
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
