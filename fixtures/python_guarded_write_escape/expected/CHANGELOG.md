# Golden Output Changes

## Pending — python_guarded_write_escape (1)

Reason:
RIPR-SPEC-0260 / #9833 / #9868: native Python old/new controls establish the literal expected class; guard entry and explicit field retention determine this fixture, with one nonempty finding.

Command:
`cargo xtask goldens bless python_guarded_write_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_guarded_write_escape (2)

Reason:
RIPR-SPEC-0108 / #9833 / #9868: produce the required full human classification projection; literal one-finding/class and anti-clean controls preserve native old/new semantics.

Command:
`cargo xtask goldens bless python_guarded_write_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
