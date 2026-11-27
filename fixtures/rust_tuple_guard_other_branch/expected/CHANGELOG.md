# Golden Output Changes

## Pending — rust_tuple_guard_other_branch (1)

Reason:
RIPR-SPEC-0443: the only input wraps 2, so the edited addition guarded by a.0 > 100 is skipped

Command:
`cargo xtask goldens bless rust_tuple_guard_other_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_tuple_guard_other_branch (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_tuple_guard_other_branch --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_tuple_guard_other_branch (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_tuple_guard_other_branch --reason "..."`

Updated:
- `expected/human-full.txt`
