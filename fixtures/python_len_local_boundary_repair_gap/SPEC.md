# Fixture: python_len_local_boundary_repair_gap

Spec: RIPR-SPEC-0028

## Given

A Python production function compares a local that measures a parameter
against a limit parameter and changes the predicate from:

```python
visible >= limit
```

to:

```python
visible > limit
```

with `visible = len(text)` bound exactly once, before the predicate, and
neither `visible` nor `text` rebound anywhere in the owner. Two strong pytest
tests call the owner with literal strings whose length is away from the limit
(`"abcdef"` against `3`, `"ab"` against `5`). This is the shape of click's
`_truncate_visible` (#11103).

The fixture workspace enables the Python preview adapter explicitly:

```toml
[languages]
enabled = ["rust", "python"]
```

## When

```bash
cargo xtask fixtures python_len_local_boundary_repair_gap
```

## Then

The Python preview adapter:

- derives `visible` as the character length of each literal string argument
  (`6` and `2`),
- classifies the changed predicate as `weakly_exposed` because no strong
  related call places `visible` at `limit`,
- names `visible == limit` as the missing discriminator, with the observed
  `visible` values in its reason,
- emits a Python repair card for the equality input shape instead of the
  generic no-repair-card Safe next action.

## Must Not

- Run pytest or import the Python project.
- Derive a length for a local that is bound twice, loop-carried, bound after
  the predicate, computed from a non-parameter, or measures a rebound
  parameter, or for a call argument that is not a plain string literal; those
  stay unresolved with no repair card (the unit controls in
  `analysis/language/python/tests.rs` pin them).
- Report the derived length as a call argument.
