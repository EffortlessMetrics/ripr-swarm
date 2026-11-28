# Fixture: python_route_response_compact_loop

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The test binds `response` to `client.get("/price")`, then rebinds it in `for response in(client.get("/other"),):`. The diff changes the handler's `total` from 190 to 195. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. The asserted response is the `/other` one, so it does not observe the changed handler. Any use of the name other than its one assignment and attribute reads refuses the read.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
