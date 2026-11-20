# Fixture: python_route_response_unpacked_options

Spec: RIPR-SPEC-0238

## Given

A FastAPI handler and a `TestClient` test over the same app that reads one key
of the route's JSON response. Confirmed false promotion from review of #14870.

## When

The application is built with `FastAPI(**{"openapi" "_url": "/price"})`, which moves the generated schema route onto `/price` under a key the source never spells; the diff changes `"openapi": "3.0.0"` to `"openapi": "3.1.0"`. The fixture input is the after-edit tree; its test is unchanged.

## Then

The changed handler has one finding that is not `exposed`. FastAPI registers the schema route while the application is constructed, before the handler, so the request answers with the generated schema, whose `openapi` value is `3.1.0` in both versions. A `**` unpack in call arguments leaves the application's options unestablished and withdraws the route-body fact.

## Must Not

Promote the handler to `exposed` through the route-response credit (#13869), or
report a clean result. RIPR-SPEC-0108 independently enforces the finding count
and `must_not_promote`, even if the generated goldens are re-blessed.
