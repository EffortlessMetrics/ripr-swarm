# Golden Output Changes

## Pending — rust_dyn_box_trait_impl (1)

Reason:
RIPR-SPEC-0001: a Box<dyn Trait> binding dispatches the trait's methods to the boxed value's impl (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_trait_impl --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_dyn_box_trait_impl (2)

Reason:
RIPR-SPEC-0001: the fixture patch drops its trailing blank context line, which changes the recorded diff identity (#15277)

Command:
`cargo xtask goldens bless rust_dyn_box_trait_impl --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_dyn_box_trait_impl (3)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when State reads no test gap found

Command:
`cargo xtask goldens bless rust_dyn_box_trait_impl --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_dyn_box_trait_impl (4)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_dyn_box_trait_impl --reason "..."`

Updated:
- `expected/human.txt`
