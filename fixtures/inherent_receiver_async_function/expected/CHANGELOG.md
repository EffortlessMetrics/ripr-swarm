# Golden Output Changes

## Pending — inherent_receiver_async_function (1)

Reason:
RIPR-SPEC-0005: initialize the new async-function helper execution regression; no existing snapshot re-bless

Command:
`cargo xtask goldens bless inherent_receiver_async_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
