# Golden Output Changes

## Pending — rust_multi_line_dropped_conjunct (1)

Reason:
RIPR-SPEC-0001: a conjunct dropped from a multi-line predicate reads infection_unknown against its whole old chain (#7945)

Command:
`cargo xtask goldens bless rust_multi_line_dropped_conjunct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
