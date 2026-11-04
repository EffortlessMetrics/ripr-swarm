# Fixture: python_family_selection_passed_over_card

Spec: RIPR-SPEC-0224

## Given

A Python function changes a return multiplier, and a pytest test both expects a
raised error and asserts a positive return value on the same changed function.

The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_family_selection_passed_over_card
```

or:

```bash
ripr check \
  --root fixtures/python_family_selection_passed_over_card/input \
  --diff fixtures/python_family_selection_passed_over_card/diff.patch \
  --mode fast
```

## Then

The Python preview adapter:

- classifies the changed return as `weakly_exposed`,
- selects the value assertion for family relevance,
- sets `alignment_reason` to `other_behavior_assertion_passed_over`,
- retains the strength-only reason in `superseded_alignment_reason`,
- keeps the repair card advisory (never delegated through the gap ledger).

## Must Not

- Promote the finding to `exposed` from the passed-over exception assertion alone.
- Delegate `other_behavior_assertion_passed_over` through the gap ledger.
