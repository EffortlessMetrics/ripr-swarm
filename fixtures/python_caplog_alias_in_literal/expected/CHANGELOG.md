# Golden Output Changes

## Pending — python_caplog_alias_in_literal (1)

Reason:
RIPR-SPEC-0238: a captured-log alias spelled inside the asserted literal is not substituted; weak gap, never exposed

Command:
`cargo xtask goldens bless python_caplog_alias_in_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_caplog_alias_in_literal (2)

Reason:
RIPR-SPEC-0238: seed human-full output for the typed honesty assertion; same weak-gap result

Command:
`cargo xtask goldens bless python_caplog_alias_in_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_alias_in_literal (3)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless python_caplog_alias_in_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
