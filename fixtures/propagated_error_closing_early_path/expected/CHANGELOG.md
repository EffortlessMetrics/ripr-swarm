# Golden Output Changes

## Pending — propagated_error_closing_early_path (1)

Reason:
RIPR-SPEC-0197: complete parser-owned propagation identity cannot certify the sibling early-error path; source ripr issue 1715 owns this closing-line slice.

Command:
`cargo xtask goldens bless propagated_error_closing_early_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — propagated_error_closing_early_path (2)

Reason:
RIPR-SPEC-0197: include the full human evidence for independent non-promotion validation of source ripr issue 1715.

Command:
`cargo xtask goldens bless propagated_error_closing_early_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
