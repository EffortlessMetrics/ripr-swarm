# Fixture: python_class_attr_text_unobserved

Spec: RIPR-SPEC-0238
Spec: RIPR-SPEC-0108

## Given

`Length` in `src/fx/core.py` carries the class attribute `message_max`, which
`__call__` formats into the `ValidationError` it raises. One test constructs
`Length` and compares an unrelated return value. The other expects the error
with `match="Longer than"`, a prefix both the old and the new text contain.

## When

The sole diff changes `message_max` from
`"Longer than maximum length {max}."` to
`"Longer than the maximum length {max}."` on line 6 of `src/fx/core.py`.

## Then

Exactly one finding is reported for line 6, owned by
`python:src/fx/core.py::Length`, classified `weakly_exposed`: the tests reach
the class, but no assertion carries a literal that the old text accepts and
the new text rejects (#15294, #15646).

```expected-classes
probe:src_fx_core.py:python_preview:v1-5f28b522 -> weakly_exposed
```

## Must Not

Promote this finding to `exposed` because a strong assertion reaches `Length`,
or erase it to read clean.
