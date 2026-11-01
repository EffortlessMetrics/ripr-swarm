# Golden Output Changes

## Pending — python_one_line_def_body_fixture (1)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: a one-line def body uses its fixture, so the boundary stays static_unknown (#10685)

Command:
`cargo xtask goldens bless python_one_line_def_body_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_one_line_def_body_fixture (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_one_line_def_body_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_one_line_def_body_fixture (3)

Reason:
RIPR-SPEC-0238: name the blocking pytest fixture in Why unknown (#10979)

Command:
`cargo xtask goldens bless python_one_line_def_body_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_one_line_def_body_fixture (4)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_one_line_def_body_fixture --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
