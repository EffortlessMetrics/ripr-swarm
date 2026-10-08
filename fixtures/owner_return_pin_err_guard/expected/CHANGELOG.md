# Golden Output Changes

## Pending — owner_return_pin_err_guard (1)

Reason:
RIPR-SPEC-0197: the owner-return pin reads a lone top-level equality from assert!(a == b) and from the twin of a != Err-return guard; matched positive and false-credit controls

Command:
`cargo xtask goldens bless owner_return_pin_err_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
