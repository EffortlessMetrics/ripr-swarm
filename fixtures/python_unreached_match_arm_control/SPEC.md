# Fixture: python_unreached_match_arm_control

Spec: RIPR-SPEC-0260

## Given

Same owner and edit as python_unreached_match_arm, but the only test calls `kind("x")`, a string that no earlier arm takes, so the wildcard arm runs under an observed input.

## When

`cargo xtask fixtures python_unreached_match_arm_control`

## Then

The finding stays `exposed`: this discriminating negative control proves the unreached-arm rule does not demote an arm an observed input reaches.

## Must Not

Promote beyond the stated class. The governed RIPR-SPEC-0108 honesty corpus
independently checks this outcome.
