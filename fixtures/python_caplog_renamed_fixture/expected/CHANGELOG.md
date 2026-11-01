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
RIPR-SPEC-0238 inherited main drift: #10444 fixture predates the #10426 evidence legend

RIPR-SPEC-0122: grace re-bless; inherited drift from #10444's caplog-override analysis interacting with #10426's legend wording; the match-guard lane cannot affect this fixture

Provenance: the legend refresh recorded in #10351 (`cae9ac0bd711c511d7ff13495313c83bea511794`)
was also incorporated into main by #10457 (`d2ae1fe658f9ec2a255f281b0f7de6f15596c201`).
This existing entry records that shared refresh; no additional output regeneration.

Command:
`cargo xtask goldens bless python_caplog_renamed_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_renamed_fixture (3)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_caplog_renamed_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_caplog_renamed_fixture (4)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_caplog_renamed_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
