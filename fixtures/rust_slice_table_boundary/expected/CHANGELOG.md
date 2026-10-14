# Golden Output Changes

## Pending — rust_slice_table_boundary (1)

Reason:
RIPR-SPEC-0001: Literal slice table lengths 1, 2, 3 miss empty input; public RED/GREEN pins the missing equality (#7553).

Command:
`cargo xtask goldens bless rust_slice_table_boundary --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
