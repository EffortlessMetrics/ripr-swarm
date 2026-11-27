# Fixture: python_async_fixture_alias_rebound

Spec: RIPR-SPEC-0238

## Given

The discount changes from ten to five. The module switches `fx` to plain `pytest.fixture` before decoration. The strict-mode test cannot rely on an awaited `Cart` instance.

## When

`cargo xtask fixtures python_async_fixture_alias_rebound`

## Then

The single finding is `static_unknown`.

```expected-classes
probe:src_cart.py:python_preview:b2d8df81 -> static_unknown
```

## Must Not

Grant constructed-receiver credit after plain pytest takes over the decorator.
