# Fixture: python_route_response_adjacent_list_tokens

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The diff changes `{"total": [5], "count": 3}` to `{"total": [2 +3], "count": 3}`. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. Python evaluates `[2 +3]` to `[5]`, so both versions pass the assertion. Two tokens of one list element are not concatenated into a value (`[23]`); the element has no normal form, so the key is not a changed key.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
