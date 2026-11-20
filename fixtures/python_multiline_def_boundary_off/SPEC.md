# Fixture: python_multiline_def_boundary_off

Spec: RIPR-SPEC-0028
Spec: RIPR-SPEC-0108

## Given

The owner `expired` in `src/fx/core.py` declares its parameters on a multi-line
`def` header (`    max_age: int | None = None,` on a continuation line) and
compares the derived local `age = now - 100` against `max_age`. The test calls
`expired(105, max_age=10)` and `expired(111, max_age=10)`: ages 5 and 11, so no
call places `age == max_age`.

## When

The sole diff changes `age > max_age` to `age >= max_age` on line 7 of
`src/fx/core.py`.

## Then

Exactly one finding is reported for line 7, owned by
`python:src/fx/core.py::expired`, classified `weakly_exposed` (#15292). The
header's continuation lines are not body rebindings of `max_age`, so the
boundary gate applies and finds no test input at the changed boundary.

## Must Not

Read the multi-line header's `max_age: int | None = None,` line as a rebinding
of `max_age`, leave the boundary activation unknown, and keep an `exposed`
verdict. Do not promote this finding to `exposed`, erase it to read clean, or
bless a promoted golden past the independent honesty-corpus oracle.
