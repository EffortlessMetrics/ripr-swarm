# Golden Output Changes

## Pending — owner_return_pin_token_direct (1)

Reason:
RIPR-SPEC-0197 (#4478): shared return-oracle execution and macro admission, matched correct/wrong-library control

Command:
`cargo xtask goldens bless owner_return_pin_token_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_token_direct (2)

Reason:
RIPR-SPEC-0197 (#4478): include full-human evidence for independent expected-class honesty assertions

Command:
`cargo xtask goldens bless owner_return_pin_token_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_token_direct (3)

Reason:
RIPR-SPEC-0197 (#4478, #5020): test-build availability, original assertion cardinality and collection guidance; preserve matched runtime controls

Command:
`cargo xtask goldens bless owner_return_pin_token_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_token_direct (4)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_token_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
