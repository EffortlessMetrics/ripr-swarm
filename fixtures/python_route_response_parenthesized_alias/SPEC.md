# Fixture: python_route_response_parenthesized_alias

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

A route alias wrapped in parentheses (`register = (app.get("/price"))`) registers `cached` on `/price` before `price`; the diff changes `price`'s `total` from 190 to 195. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. Starlette dispatches to the first matching route, so `cached` answers and its 190 satisfies the assertion in both versions. A parenthesized route-factory assignment is an indirect registration, which makes the workspace opaque.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
