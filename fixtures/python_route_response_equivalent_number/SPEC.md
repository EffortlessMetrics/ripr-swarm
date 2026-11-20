# Fixture: python_route_response_equivalent_number

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler returns `{"total": [5], "count": 3}`. A `TestClient` test over
the same app requests the route and asserts `response.json()["total"] == [5]`.

## When

The diff changes the handler's returned dict from `{"total": [5], "count": 3}` to `{"total": [5.0], "count": 3}`.
The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. `[5]` and `[5.0]`
decode to lists that compare equal in Python, so both versions pass the
assertion.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. A key whose old and new values give one JSON body to a
Python comparison is not a changed key for the read. RIPR-SPEC-0108 independently enforces the finding count and
`must_not_promote`, even if the generated goldens are re-blessed.
