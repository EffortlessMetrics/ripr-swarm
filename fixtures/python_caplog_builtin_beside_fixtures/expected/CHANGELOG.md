# Golden Output Changes

## Pending — python_caplog_builtin_beside_fixtures (1)

Reason:
RIPR-SPEC-0238: builtin caplog beside unrelated fixtures stays credited (#10418)

Command:
`cargo xtask goldens bless python_caplog_builtin_beside_fixtures --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_builtin_beside_fixtures (2)

Reason:
RIPR-SPEC-0122: grace re-bless; inherited drift from #10444's caplog-override analysis interacting with #10426's legend wording; the match-guard lane cannot affect this fixture

Command:
`cargo xtask goldens bless python_caplog_builtin_beside_fixtures --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
