# Golden Output Changes

## Pending — rust_dyn_box_default_not_overridden_box (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> call on a type that keeps the changed default reaches it beside an override (#15752)

Command:
`cargo xtask goldens bless rust_dyn_box_default_not_overridden_box --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
