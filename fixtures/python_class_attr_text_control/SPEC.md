# Fixture: python_class_attr_text_control

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

The same `Length` class as `python_class_attr_text_unobserved`. The error
test binds the raised error and asserts, in one parenthesized `assert` that
spans four lines, `len(exc.value.args) == 1 and str(exc.value) == "Longer than
maximum length 1."`.

## When

The sole diff changes `message_max` from
`"Longer than maximum length {max}."` to
`"Longer than the maximum length {max}."` on line 6 of `src/fx/core.py`.

## Then

Exactly one finding is reported for line 6, owned by
`python:src/fx/core.py::Length`, classified `exposed`: the second conjunct
must hold for the test to pass, and it equals the old text only (#15646).

```expected-classes
probe:src_fx_core.py:python_preview:v1-5f28b522 -> exposed
```

## Must Not

Demote this finding below `exposed` because the equality is a conjunct or
because the statement spans lines, or erase it to read clean.
