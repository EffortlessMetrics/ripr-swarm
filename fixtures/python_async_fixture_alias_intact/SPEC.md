# Fixture: python_async_fixture_alias_intact

Spec: RIPR-SPEC-0238

## Given

The discount changes from ten to five. The `fx` alias retains its original `pytest_asyncio.fixture` binding. The test receives the constructed `Cart` and its assertion distinguishes the changed return.

## When

`cargo xtask fixtures python_async_fixture_alias_intact`

## Then

The single finding is `exposed`.

```expected-classes
probe:src_cart.py:python_preview:b2d8df81 -> exposed
```

## Must Not

Withhold constructed-receiver credit while the asyncio provider remains intact.
