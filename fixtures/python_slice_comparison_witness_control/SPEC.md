# Fixture: python_slice_comparison_witness_control

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The same `is_musl(tag)` as `python_slice_comparison_no_witness`. One test
calls it with `"musx"`, whose first three characters are `"mus"` and whose
first four are not `"musl"`, and asserts `True`.

## When

The sole diff changes `tag[:4] != "musl"` to `tag[:3] != "mus"` on line 2 of
`src/fx/core.py`.

## Then

Exactly one finding is reported for line 2, owned by
`python:src/fx/core.py::is_musl`, classified `exposed`: `"musx"` gives the two comparisons different results,
and the exact assertion passes only after the edit.

```expected-classes
probe:src_fx_core.py:python_preview:d40e537d -> exposed
```

## Must Not

Demote this finding below `exposed` because the compared operand is a slice,
or erase it to read clean.
