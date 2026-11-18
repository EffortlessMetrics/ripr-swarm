# Golden Output Changes

## Pending — rust_computed_return_tail_wrapping_local (1)

Reason:
RIPR-SPEC-0443: a wrapping narrow local must not credit the reached-tail pin (#14950 review)

Command:
`cargo xtask goldens bless rust_computed_return_tail_wrapping_local --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
