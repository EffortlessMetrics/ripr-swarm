# Fixture: python_route_response_json_patch

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler returns `{"total": 200 - 10, "count": 3}`. A `TestClient`
test over the same app requests the route, replaces the response's `json`
method in a tuple assignment with one returning `{"total": 190}`, and asserts
`response.json()["total"] == 190`.

## When

The diff changes the handler's returned dict from `{"total": 200 - 10, "count": 3}` to `{"total": 200 - 5, "count": 3}`.
The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. The assertion reads
the replacement method, not the handler's body, so both versions pass it.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. Any `.json` that is not called at once (here a tuple
target) refuses the response read. RIPR-SPEC-0108 independently enforces the finding count and
`must_not_promote`, even if the generated goldens are re-blessed.
