# Golden Output Changes

## Pending — rust_closed_if_statement_tail (1)

Reason:
RIPR-SPEC-0108: the line after a self-closed if statement is not its branch value (#10880)

Command:
`cargo xtask goldens bless rust_closed_if_statement_tail --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
