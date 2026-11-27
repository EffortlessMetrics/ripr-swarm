# Golden Output Changes

## Pending — rust_dyn_box_default_overridden (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> call on a type that overrides the changed default does not reach it (#15752)

Command:
`cargo xtask goldens bless rust_dyn_box_default_overridden --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_dyn_box_default_overridden (2)

Reason:
RIPR-SPEC-0001: rebless for main's why-unrevealed wording (#15752)

Command:
`cargo xtask goldens bless rust_dyn_box_default_overridden --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_dyn_box_default_overridden (3)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_dyn_box_default_overridden --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_dyn_box_default_overridden (4)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_dyn_box_default_overridden --reason "..."`

Updated:
- `expected/human-full.txt`
