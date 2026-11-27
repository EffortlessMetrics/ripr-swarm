# Golden Output Changes

## Pending — rust_trait_inherent_twin (1)

Reason:
RIPR-SPEC-0108: a call that dispatches to an override or an inherent twin is name-only, not direct_owner_call (#15273)

Command:
`cargo xtask goldens bless rust_trait_inherent_twin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_inherent_twin (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_trait_inherent_twin --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_trait_inherent_twin (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_trait_inherent_twin --reason "..."`

Updated:
- `expected/human-full.txt`
