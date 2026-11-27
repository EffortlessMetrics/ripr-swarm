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

## Pending — rust_computed_return_tail_wrapping_local (2)

Reason:
RIPR-SPEC-0122: the top-gap Safe next action points at the Next step the digest prints (#14173)

Command:
`cargo xtask goldens bless rust_computed_return_tail_wrapping_local --reason "..."`

Updated:
- `expected/human.txt`
