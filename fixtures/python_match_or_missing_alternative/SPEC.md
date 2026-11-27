# Fixture: python_match_or_missing_alternative

Spec: RIPR-SPEC-0260

## Given

A Python match arm drops `"y"` from `case "yes" | "y"`. The parametrized
test pins the return value for `"yes"` and `"on"`. Both inputs select the
same arm before and after the edit.

## When

`cargo xtask fixtures python_match_or_missing_alternative`

## Then

The predicate reads `weakly_exposed` and names `val == "y"`.

```expected-classes
probe:src_answer.py:python_preview:20b23ed9 -> weakly_exposed
```

## Must Not

Give strong credit to an unchanged alternative or to the fallback row.
