# Fixture: python_caplog_renamed_fixture

Spec: RIPR-SPEC-0238

## Given

The test requests `caplog` and asserts `"low balance: 5" in caplog.text`, but
`tests/conftest.py` defines `@pytest.fixture(name="caplog")` that returns a
fixed recorder.

## When

The logged message changes from `low balance: %s` to `balance low: %s`.

## Then

The test reads a recorder, not pytest's capture, so ripr cannot tell whether
the edit changes what it reads: the finding is `static_unknown`, never
`exposed` or a repair gap.

## Must Not

Evaluate the logged text against a `caplog` that is not pytest's built-in
fixture (rule 22, #10418).
