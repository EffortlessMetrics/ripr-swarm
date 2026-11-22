# Golden Output Changes

## Pending — rust_trait_default_overridden (1)

Reason:
RIPR-SPEC-0108: a call that dispatches to an override or an inherent twin is name-only, not direct_owner_call (#15273)

Command:
`cargo xtask goldens bless rust_trait_default_overridden --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_default_overridden (2)

Reason:
RIPR-SPEC-0122: the Summary names findings whose reach is a test link ripr could not confirm (#15295)

Command:
`cargo xtask goldens bless rust_trait_default_overridden --reason "..."`

Updated:
- `expected/human.txt`
