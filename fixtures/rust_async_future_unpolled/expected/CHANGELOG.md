# Golden Output Changes

## Pending — rust_async_future_unpolled (1)

Reason:
RIPR-SPEC-0240 #13702: initial bless; a future created but never polled is a named limit (rust_future_never_polled)

Command:
`cargo xtask goldens bless rust_async_future_unpolled --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_async_future_unpolled (2)

Reason:
union-head re-bless after main merge: additive evidence_stages rows (RIPR-SPEC-0429)

Command:
`cargo xtask goldens bless rust_async_future_unpolled --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
