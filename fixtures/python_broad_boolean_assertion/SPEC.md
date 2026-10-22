# Fixture: python_broad_boolean_assertion

Spec: RIPR-SPEC-0028

## Given

A Python production function changes a predicate boundary, and a pytest test
calls the changed owner with a broad boolean assertion on a non-literal
argument. (With literal arguments and a provably `bool`-returning owner, the
assertion is an exact pin; see the py-grid-*-exact verdict-corpus cases, #9542.)

The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_broad_boolean_assertion
```

or:

```bash
ripr check \
  --root fixtures/python_broad_boolean_assertion/input \
  --diff fixtures/python_broad_boolean_assertion/diff.patch \
  --mode fast
```

## Then

The Python preview adapter:

- finds the `is_priority` function owner,
- finds the pytest `test_priority_amount` related test,
- records `assert expr` as `smoke_only` / `smoke`,
- emits Python preview metadata,
- keeps the finding `weakly_exposed` because smoke evidence is not a strong
  discriminator.

## Must Not

- Execute pytest.
- Claim broad boolean evidence is exact-value evidence.
- Change Rust default behavior.
