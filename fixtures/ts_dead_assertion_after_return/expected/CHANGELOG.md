# Golden Output Changes

## Pending — ts_dead_assertion_after_return (1)

Reason:
RIPR-SPEC-0243: new fixture for the dead assertion after an unconditional return false promotion (#9003)

Command:
`cargo xtask goldens bless ts_dead_assertion_after_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
