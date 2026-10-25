# Golden Output Changes

## Pending — python_caplog_renamed_fixture (1)

Reason:
RIPR-SPEC-0238: renamed caplog fixture is an override (#10418)

Command:
`cargo xtask goldens bless python_caplog_renamed_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_renamed_fixture (2)

Reason:
RIPR-SPEC-0122: grace re-bless; inherited drift from #10444's caplog-override analysis interacting with #10426's legend wording; the match-guard lane cannot affect this fixture

Command:
`cargo xtask goldens bless python_caplog_renamed_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
