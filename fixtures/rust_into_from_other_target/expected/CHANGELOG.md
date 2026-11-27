# Golden Output Changes

## Pending — rust_into_from_other_target (1)

Reason:
RIPR-SPEC-0001: a .into() that Rust does not dispatch to the changed From impl keeps its file-only relation (#15277)

Command:
`cargo xtask goldens bless rust_into_from_other_target --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_other_target (2)

Reason:
RIPR-SPEC-0001: refresh human summary wording after merging main; no classification change (#15277)

Command:
`cargo xtask goldens bless rust_into_from_other_target --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_into_from_other_target (3)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_into_from_other_target --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_other_target (4)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_into_from_other_target --reason "..."`

Updated:
- `expected/human-full.txt`
