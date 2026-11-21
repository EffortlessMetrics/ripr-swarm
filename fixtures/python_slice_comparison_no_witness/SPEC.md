# Fixture: python_slice_comparison_no_witness

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

`is_musl(tag)` in `src/fx/core.py` returns `False` unless `tag[:4]` is
`"musl"`. The tests call it with two module constants,
`"musllinux_1_1_x86_64"` and `"manylinux_2_17_x86_64"`.

## When

The sole diff changes `tag[:4] != "musl"` to `tag[:3] != "mus"` on line 2 of
`src/fx/core.py`.

## Then

Exactly one finding is reported for line 2, owned by
`python:src/fx/core.py::is_musl`, classified `weakly_exposed`: both inputs give the old and the new comparison
the same result, so pytest passes before and after the edit (#15287). The
finding names the missing input as
`(tag[:3] == "mus") != (tag[:4] == "musl")`.

```expected-classes
probe:src_fx_core.py:python_preview:d40e537d -> weakly_exposed
```

## Must Not

Promote this finding to `exposed` because exact assertions reach `is_musl`
on both sides of the comparison, or erase it to read clean.
