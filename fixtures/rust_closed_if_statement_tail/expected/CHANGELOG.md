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

## Pending — rust_closed_if_statement_tail (2)

Reason:
RIPR-SPEC-0108: a self-closed if statement no longer lends the next line as a branch value (#10880); rebased on main's reworded human output

Command:
`cargo xtask goldens bless rust_closed_if_statement_tail --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_closed_if_statement_tail (3)

Reason:
RIPR-SPEC-0108: plain wording 'spot' for 'seam' in the check/repair disagreement note (#10880)

Command:
`cargo xtask goldens bless rust_closed_if_statement_tail --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
