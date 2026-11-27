# Golden Output Changes

## Pending — rust_deref_receiver_owner (1)

Reason:
RIPR-SPEC-0001: an inherent method called through the one std Deref wrapper of its type runs the changed body (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_deref_receiver_owner (2)

Reason:
RIPR-SPEC-0001: the Deref wrapper reading shares the closed-dispatch workspace verdict and its fixtures drop the trailing blank diff line and the probe member name (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_owner --reason "..."`

Updated:
- `expected/check.json`

## Pending — rust_deref_receiver_owner (3)

Reason:
RIPR-SPEC-0001: summary wording from main for an exposed-only report (#15277)

Command:
`cargo xtask goldens bless rust_deref_receiver_owner --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_deref_receiver_owner (4)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_deref_receiver_owner --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_deref_receiver_owner (5)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_deref_receiver_owner --reason "..."`

Updated:
- `expected/human-full.txt`
