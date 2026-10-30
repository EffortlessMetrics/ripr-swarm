# Fixture: python_caplog_builtin_beside_fixtures

Spec: RIPR-SPEC-0238

## Given

The test uses pytest's own `caplog` and asserts `"low balance: 5" in
caplog.text`. `tests/conftest.py` defines unrelated fixtures: one renamed to
`recorder`, and one whose `params=` list spells `name="caplog"` as data.

## When

The logged message changes from `low balance: %s` to `balance low: %s`.

## Then

Neither fixture overrides `caplog`, so the changed message is read and the
finding is `exposed`.

## Must Not

Treat text in a decorator argument, or an unrelated rename, as a `caplog`
override and abstain (rule 22, #10418).
