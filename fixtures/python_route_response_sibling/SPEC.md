# Fixture: python_route_response_sibling

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler returns `{"total": 200 - 10, "meta": {"count": 3}}`. A
`TestClient` test over the same app requests the route and asserts only the
unchanged sibling field, `response.json()["meta"] == {"count": 3}`.

## When

The diff changes the returned `total` from `200 - 10` to `200 - 5`.
The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. The test reaches the
handler, but its assertion compares a field the edit does not touch, so both
versions pass it.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. The response operand must read a changed key; a
whole-collection comparison of a sibling field is not one. RIPR-SPEC-0108
independently enforces the finding count and `must_not_promote`, even if the
generated goldens are re-blessed.
