# Golden Output Changes

## Pending — owner_return_pin_err_guard_shadowed_err (1)

Reason:
RIPR-SPEC-0197: seed new shadowed-Err wrong-implementation control from #7063 review

Command:
`cargo xtask goldens bless owner_return_pin_err_guard_shadowed_err --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_err_guard_shadowed_err (2)

Reason:
RIPR-SPEC-0197: inherit main's agent-stub --kind output format in the human.txt golden

Command:
`cargo xtask goldens bless owner_return_pin_err_guard_shadowed_err --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
