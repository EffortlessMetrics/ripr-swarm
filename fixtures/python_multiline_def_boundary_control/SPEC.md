# Fixture: python_multiline_def_boundary_control

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The owner `over` in `src/fx/core.py` declares `age` and `max_age` on a
multi-line `def` header and compares `age > max_age`. The test calls
`over(10, max_age=10)`, which places the boundary, and `over(11, max_age=10)`.

## When

The sole diff changes `age > max_age` to `age >= max_age` on line 5 of
`src/fx/core.py`.

## Then

Exactly one finding is reported for line 5, owned by
`python:src/fx/core.py::over`, classified `exposed`: skipping the header
(#15292) still credits a test that places the boundary.

```expected-classes
probe:src_fx_core.py:python_preview:9e50cd52 -> exposed
```

## Must Not

Demote this finding below `exposed` because the parameters sit on a multi-line
header, or erase it to read clean.
