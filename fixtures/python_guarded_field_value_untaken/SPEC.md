# Fixture: python_guarded_field_value_untaken

Spec: RIPR-SPEC-0260

Governing issue: #13621

## Given

`order_view` returns `{"status": 400}` inside `if total < 0:`; the change replaces it with `{"status": 422}`. The only related test calls `order_view(None, 100)` and asserts the 200 response, so no test input satisfies the guard.

## When

```bash
cargo xtask fixtures python_guarded_field_value_untaken
```

## Then

The changed field value reads `weakly_exposed` and names `total < 0` as the missing input. A strong exact-value oracle on inputs that never enter the guarded branch does not pin the changed value. A test that calls `order_view(None, -1)` and asserts 422 would read `exposed` (unit test `guarded_field_value_with_entering_input_stays_exposed`).

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:orders_views.py:python_preview:11327452 -> weakly_exposed
```

## Must Not

- Credit a changed value in a branch no observed input enters as `exposed`.
- Claim runtime mutation adequacy.
