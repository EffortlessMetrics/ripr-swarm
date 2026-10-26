# Fixture: python_route_rival_app

Spec: RIPR-SPEC-0238

## Given

Two modules each define `app` and register `@app.post("/checkout")`:
`shop/orders.py` (the changed handler) and `shop/admin.py`.

The only pytest test builds `TestClient(app)` from `from shop.admin import
app` and calls `client.post("/checkout")`. Path and verb match the changed
handler, but the client targets the other module's application.
The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_route_rival_app
```

or:

```bash
ripr check \
  --root fixtures/python_route_rival_app/input \
  --diff fixtures/python_route_rival_app/diff.patch \
  --mode fast
```

## Then

The Python preview adapter:

- does not link the `client.post` call to the changed `shop/orders.py` route
  owner, because the client is built over a same-named application imported
  from another module,
- does not credit the test's `response.json()["detail"]` assertion to the
  changed handler,
- reports the changed handler as `no_static_path`.

It does not resolve route registration, mounts or prefixes, and a client it
cannot identify (a fixture, an unresolved name) keeps the path-and-verb match.

## Must Not

- Execute imports, route registration, pytest, or the route handler.
- Match a route on path and verb alone when the client is built over a
  same-named application imported from another module.
- Promote Python out of preview/advisory status.
