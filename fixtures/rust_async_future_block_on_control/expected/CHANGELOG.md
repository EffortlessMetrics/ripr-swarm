# Golden Output Changes

## Pending — rust_async_future_block_on_control (1)

Reason:
RIPR-SPEC-0240 #13702: initial bless; block_on control keeps reachable_unrevealed

Command:
`cargo xtask goldens bless rust_async_future_block_on_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_async_future_block_on_control (2)

Reason:
union-head re-bless after main merge: additive evidence_stages rows (RIPR-SPEC-0429)

Command:
`cargo xtask goldens bless rust_async_future_block_on_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
