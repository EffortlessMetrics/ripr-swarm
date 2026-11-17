# Fixture: python_route_response_replaced_content

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The test replaces `response._content` with `b'{"total": 190}'` before reading `response.json()`. The diff changes the handler's `total` from 190 to 195. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. The decoded body is test-written data, so both versions pass the assertion. An attribute write on the response refuses the read.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
