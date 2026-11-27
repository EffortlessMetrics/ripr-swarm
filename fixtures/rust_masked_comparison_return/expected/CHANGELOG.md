# Golden Output Changes

## Pending — rust_masked_comparison_return (1)

Reason:
RIPR-SPEC-0158: an unchanged false gate masks the returned comparison on both asserted inputs (#14709)

Command:
`cargo xtask goldens bless rust_masked_comparison_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_masked_comparison_return (2)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when State reads no test gap found

Command:
`cargo xtask goldens bless rust_masked_comparison_return --reason "..."`

Updated:
- `expected/human.txt`
