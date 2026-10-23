# Fixture: python_protocol_entry_bracketed_rebind_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`numeric_range.__reversed__` changes its empty guard. The original test builds
`r = numeric_range(0)`, then rebinds `r` with the spaced-and-nested target
`((r), x) = [1], 2` before asserting `list(reversed(r)) == [1]`.

Two further tests replace `r` through the non-adjacent targets
`(x, (r)) = 2, [1]` and `[a, [r]] = [2, [[1]]]` (#9531).

Four review controls cover an unbracketed outer tuple, a chained outer
assignment, and inline loop/with suites. Each must remain uncredited.

## When

`cargo xtask fixtures python_protocol_entry_bracketed_rebind_no_exposed_credit`

## Then

The finding is not `exposed`: `r` no longer holds the constructed
`numeric_range`, so `reversed(r)` does not reach the changed `__reversed__`
and the protocol-entry relation is withheld. These are the #9178 and #9531
false-promotion controls.

## Must Not

Credit the rebound receiver as a protocol entry of the owner. The governed
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
