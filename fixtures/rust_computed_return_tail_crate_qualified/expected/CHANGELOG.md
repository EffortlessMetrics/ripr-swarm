# Golden Output Changes

## Pending — rust_computed_return_tail_crate_qualified (1)

Reason:
RIPR-SPEC-0443: an own-crate qualified exact pin on a reached computed tail is read as the bare owner call; a rebound crate name is not (#13037)

Command:
`cargo xtask goldens bless rust_computed_return_tail_crate_qualified --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_computed_return_tail_crate_qualified (2)

Reason:
RIPR-SPEC-0443: add the human-full projection for the crate-qualified pin cases (#13037)

Command:
`cargo xtask goldens bless rust_computed_return_tail_crate_qualified --reason "..."`

Updated:
- `expected/human-full.txt`
