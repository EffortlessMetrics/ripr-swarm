# Fixture: python_route_verb_mismatch

Spec: RIPR-SPEC-0028

## Given

A Python route handler is registered with `@api.post("/checkout")` and returns
a JSON-shaped dict field from a changed branch.

The only pytest test calls `client.get("/checkout")`. The path matches the
route literal but the HTTP verb does not match the decorator.
The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_route_verb_mismatch
```

or:

```bash
ripr check \
  --root fixtures/python_route_verb_mismatch/input \
  --diff fixtures/python_route_verb_mismatch/diff.patch \
  --mode fast
```

## Then

The Python preview adapter:

- does not link the `client.get` call to the `@api.post` route owner,
- does not credit the test's `response.json()["detail"]` assertion to the
  changed handler,
- reports the changed field with no related test.

A `client.post("/checkout")` call, the same path with the decorator's verb,
still links (`python_api_json_field_repair_gap`).

## Must Not

- Execute imports, route registration, pytest, or the route handler.
- Match a route on the path literal alone when the decorator names a verb.
- Promote Python out of preview/advisory status.
