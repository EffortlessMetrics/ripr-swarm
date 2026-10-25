# Fixture: python_protocol_entry_tripled_quote_header_rebind_no_exposed_credit

Spec: RIPR-SPEC-0028

## Given

`numeric_range.__reversed__` changes its empty guard. The only test builds
`r = numeric_range(0)`, then rebinds `r` in a one-line suite after a header holding a
tripled-quote string,
`if z == '''it's''': ( r), x = [1], 2`, before asserting `list(reversed(r)) == [1]`.

## When

`cargo xtask fixtures python_protocol_entry_tripled_quote_header_rebind_no_exposed_credit`

## Then

The finding is not `exposed`: `r` no longer holds the constructed
`numeric_range`, so `reversed(r)` does not reach the changed `__reversed__`
and the protocol-entry relation is withheld. This is the #9992
false-promotion correction.

## Must Not

Credit the rebound receiver as a protocol entry of the owner. The governed
RIPR-SPEC-0108 honesty corpus independently forbids promotion.
