# Golden Output Changes

## Pending — owner_return_pin_trait_method (1)

Reason:
RIPR-SPEC-0197: initial golden for owner-return pins (#4478)

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_trait_method (2)

Reason:
RIPR-SPEC-0197 (#4478) acceptance fixture re-rendered in main's #4520 exposure-word format: the trait default method stays pinned exposed through the byte-slice receiver

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_trait_method (3)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless owner_return_pin_trait_method --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
