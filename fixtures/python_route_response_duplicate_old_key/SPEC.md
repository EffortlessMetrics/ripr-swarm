# Fixture: python_route_response_duplicate_old_key

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The diff removes the first of two `total` keys: `{"total": 0, "total": 190, "count": 3}` becomes `{"total": 190, "count": 3}`. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. Python keeps the last of a duplicate key, so both versions return `total` 190. An old dict with a duplicate key has no effective mapping for the read, so no key is a changed key.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
