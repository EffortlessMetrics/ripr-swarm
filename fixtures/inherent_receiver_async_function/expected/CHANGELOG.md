# Golden Output Changes

## Pending — inherent_receiver_async_function (1)

Reason:
RIPR-SPEC-0005: initialize the new async-function helper execution regression; no existing snapshot re-bless

Command:
`cargo xtask goldens bless inherent_receiver_async_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — inherent_receiver_async_function (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_async_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
