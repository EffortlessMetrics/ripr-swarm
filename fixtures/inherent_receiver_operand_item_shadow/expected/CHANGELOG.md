# Golden Output Changes

## Pending — inherent_receiver_operand_item_shadow (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: register all three output surfaces for the independently pinned item-shadow refusal (#8004).

Command:
`cargo xtask goldens bless inherent_receiver_operand_item_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inherent_receiver_operand_item_shadow (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_operand_item_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
