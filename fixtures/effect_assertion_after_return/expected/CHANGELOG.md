# Golden Output Changes

## Pending — effect_assertion_after_return (1)

Reason:
RIPR-SPEC-0197: effect-family assertion that never runs stays reachable_unrevealed (#8927)

Command:
`cargo xtask goldens bless effect_assertion_after_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — effect_assertion_after_return (2)

Reason:
RIPR-SPEC-0197: add human-full projection for the effect-assertion honesty corpus (#8927)

Command:
`cargo xtask goldens bless effect_assertion_after_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — effect_assertion_after_return (3)

Reason:
RIPR-SPEC-0197 and #8954: replace shifted tail-return fallback with genuine aligned line18 semicolon CallDeletion and SideEffect; preserve dead assertion refusal.

Command:
`cargo xtask goldens bless effect_assertion_after_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
