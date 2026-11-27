# Golden Output Changes

## Pending — rust_dyn_box_alias_impl (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> binding dispatches the trait's methods to the boxed value's impl (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_alias_impl --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_dyn_box_alias_impl (2)

Reason:
RIPR-SPEC-0001: the fixture patch drops its trailing blank context line, which changes the recorded diff identity (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_alias_impl --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_dyn_box_alias_impl (3)

Reason:
RIPR-SPEC-0001: rebless the dyn-box fixtures for main's reached-but-not-noticed summary wording (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_alias_impl --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_dyn_box_alias_impl (4)

Reason:
RIPR-SPEC-0001: rebless the dyn-box fixtures for main's why-unrevealed wording (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_alias_impl --reason "..."`

Updated:
- `expected/human.txt`
