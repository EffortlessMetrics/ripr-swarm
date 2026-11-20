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
