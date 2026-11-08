# Fixture: python_unreached_match_arm_control

Spec: RIPR-SPEC-0260

## Given

Same owner and edit as python_unreached_match_arm, but the only test calls `kind("x")`, a string that no earlier arm takes, so the wildcard arm runs under an observed input.

## When

`cargo xtask fixtures python_unreached_match_arm_control`

## Then

The finding stays `exposed`: this discriminating negative control proves the unreached-arm rule does not demote an arm an observed input reaches.

The expected static class per finding (machine-checked against
`expected/check.json` by `check-fixture-contracts`, #12928):

```expected-classes
probe:src_kind.py:python_preview:5f2246ab -> exposed
```

## Must Not

Promote beyond the stated class. The governed RIPR-SPEC-0108 honesty corpus
independently checks this outcome.
