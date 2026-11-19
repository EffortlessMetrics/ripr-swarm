# Fixture: python_route_response_float_floor_division

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler returns `{"total": 10, "count": 3}`. A `TestClient` test over
the same app requests the route and asserts `response.json()["total"] == 10`.

## When

The diff changes the handler's returned dict from `{"total": 10, "count": 3}`
to `{"total": 3.3 // 0.3, "count": 3}`. The fixture input is the after-edit
tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. Python evaluates
`3.3 // 0.3` to `10.0` (float floor division is not `floor(3.3 / 0.3)`, which
is 11), so both versions pass the assertion.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. A float floor division, remainder or power has no
normal form for the read, so the key is not a changed key. This is the
confirmed false promotion found in review of #14870. RIPR-SPEC-0108
independently enforces the finding count and `must_not_promote`, even if the
generated goldens are re-blessed.
