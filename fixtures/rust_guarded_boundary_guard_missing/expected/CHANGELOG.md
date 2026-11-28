# Golden Output Changes

## Pending — rust_guarded_boundary_guard_missing (1)

Reason:
RIPR-SPEC-0001: a guarded boundary row that misses its guard reads infection_unknown with the guard named, not exposed through the compound reading (#13037)

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_guarded_boundary_guard_missing (2)

Reason:
RIPR-SPEC-0001: add the human-full projection of the guarded-boundary refusal (#13037)

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_guarded_boundary_guard_missing (3)

Reason:
RIPR-SPEC-0158: a single-comparison compound predicate's gap id names its one comparison (#14719); the bool guard no longer joins the discriminator

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_guarded_boundary_guard_missing (4)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_guarded_boundary_guard_missing (5)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_guarded_boundary_guard_missing --reason "..."`

Updated:
- `expected/human-full.txt`
