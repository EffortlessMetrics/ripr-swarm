# Golden Output Changes

## Pending — inherent_receiver_async_operand_helper (1)

Reason:
RIPR-SPEC-0005: initialize discarded unpolled free-helper operand regression; no existing snapshot re-bless

Command:
`cargo xtask goldens bless inherent_receiver_async_operand_helper --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
