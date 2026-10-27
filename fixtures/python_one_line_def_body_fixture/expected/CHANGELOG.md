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
