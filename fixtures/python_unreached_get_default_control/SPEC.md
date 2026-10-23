# Fixture: python_unreached_get_default_control

Spec: RIPR-SPEC-0260

## Given

Same owner and edit as python_unreached_get_default, but the only test looks up `"platinum"`, a key the table lacks, so the default runs.

## When

`cargo xtask fixtures python_unreached_get_default_control`

## Then

The finding stays `exposed`: this discriminating negative control proves the unreached-default rule does not demote a default an observed key reaches.

## Must Not

Promote beyond the stated class. The governed RIPR-SPEC-0108 honesty corpus
independently checks this outcome.
