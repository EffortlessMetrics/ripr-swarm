# Golden Output Changes

## Pending — rust_deref_receiver_alias_std_trait (1)

Reason:
RIPR-SPEC-0001: a Deref wrapper call is credited only when nothing on the wrapper may answer it first (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_deref_receiver_alias_std_trait (2)

Reason:
RIPR-SPEC-0001: the Deref wrapper reading shares the closed-dispatch workspace verdict and its fixtures drop the trailing blank diff line and the probe member name (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_deref_receiver_alias_std_trait (3)

Reason:
RIPR-SPEC-0001: main's summary wording

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_deref_receiver_alias_std_trait (4)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_deref_receiver_alias_std_trait (5)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_deref_receiver_alias_std_trait --reason "..."`

Updated:
- `expected/human-full.txt`
