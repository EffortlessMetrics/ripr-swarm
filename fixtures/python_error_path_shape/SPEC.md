# Fixture: python_error_path_shape

Spec: RIPR-SPEC-0028

## Given

A Python production function changes an error-path raise statement, and a
pytest test calls the owner inside `pytest.raises(...)`.

The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_error_path_shape
```

or:

```bash
ripr check \
  --root fixtures/python_error_path_shape/input \
  --diff fixtures/python_error_path_shape/diff.patch \
  --mode fast
```

## Then

The Python preview adapter:

- finds the `require_positive` function owner,
- classifies the changed line as an `error_path` / `control` probe,
- records `pytest.raises(ValueError)` as an exact error variant: the swap is
  between two disjoint builtins and the literal input reaches the raise
  (RIPR-SPEC-0238 rule 6, #13869),
- emits Python preview metadata.

## Must Not

- Execute pytest.
- Resolve exception inheritance beyond the fixed builtin hierarchy, or runtime
  imports.
- Treat broad error evidence (`pytest.raises(Exception)`) as a strong
  discriminator.
