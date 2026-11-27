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
