# Golden Output Changes

## Pending — rust_async_future_awaited_control (1)

Reason:
RIPR-SPEC-0240 #13702: initial bless; awaited tokio::test control keeps reachable_unrevealed

Command:
`cargo xtask goldens bless rust_async_future_awaited_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_async_future_awaited_control (2)

Reason:
union-head re-bless after main merge: additive evidence_stages rows (RIPR-SPEC-0429)

Command:
`cargo xtask goldens bless rust_async_future_awaited_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_async_future_awaited_control (3)

Reason:
RIPR-SPEC-0122: the top-gap Safe next action points at the Next step in plain words (#14173); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_async_future_awaited_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
