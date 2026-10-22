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

## Pending — ts_dead_assertion_after_return (2)

Reason:
RIPR-SPEC-0320: fixture input drops the owner's boolean return annotation so the truthiness check stays smoke-only; only input and source digests change

Command:
`cargo xtask goldens bless ts_dead_assertion_after_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
