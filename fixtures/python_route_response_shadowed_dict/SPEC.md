# Fixture: python_route_response_shadowed_dict

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The module defines `class dict(BaseModel)` and annotates the handler `-> dict`; the diff changes `"total": "1"` to `"total": 1`. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. FastAPI reads the annotation as a response model, which coerces `"1"` to `1`, so both versions pass the assertion. A source that rebinds `dict` withdraws the route-body fact.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
