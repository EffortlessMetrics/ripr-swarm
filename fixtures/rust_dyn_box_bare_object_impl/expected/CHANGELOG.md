# Golden Output Changes

## Pending — rust_dyn_box_bare_object_impl (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> binding dispatches the trait's methods to the boxed value's impl (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_bare_object_impl --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
