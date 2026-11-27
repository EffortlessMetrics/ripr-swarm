# Golden Output Changes

## Pending — rust_trait_impl_std_into_iter_takes_call (1)

Reason:
RIPR-SPEC-0108: a standard trait's by-value method of the name takes the call before a &self trait impl method (#16040)

Command:
`cargo xtask goldens bless rust_trait_impl_std_into_iter_takes_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_impl_std_into_iter_takes_call (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_trait_impl_std_into_iter_takes_call --reason "..."`

Updated:
- `expected/human.txt`
