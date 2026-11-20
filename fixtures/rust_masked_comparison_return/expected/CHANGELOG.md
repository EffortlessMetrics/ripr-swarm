# Golden Output Changes

## Pending — rust_masked_comparison_return (1)

Reason:
RIPR-SPEC-0158: an unchanged false gate masks the returned comparison on both asserted inputs (#14709)

Command:
`cargo xtask goldens bless rust_masked_comparison_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
