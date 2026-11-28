# Fixture: python_route_response_list_subscript

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The diff changes `{"total": 10, "count": 3}` to `{"total": [10][0], "count": 3}`. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. Python evaluates `[10][0]` to `10`, so both versions pass the assertion. A subscript has no normal form for the read, so the key is not a changed key.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
