# Golden Output Changes

## Pending — typescript_reexport_unparsed_star (1)

Reason:
RIPR-SPEC-0095 and RIPR-SPEC-0108: unavailable star export facts cannot establish unique owner evidence; baseline CLI falsely reported exposed with one relation.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
