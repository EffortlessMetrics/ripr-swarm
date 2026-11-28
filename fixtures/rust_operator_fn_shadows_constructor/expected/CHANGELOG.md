# Golden Output Changes

## Pending — rust_operator_fn_shadows_constructor (1)

Reason:
RIPR-SPEC-0001: a conversion or operator Rust does not dispatch to the changed impl keeps its old relation (#15277 review)

Command:
`cargo xtask goldens bless rust_operator_fn_shadows_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_operator_fn_shadows_constructor (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_operator_fn_shadows_constructor --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_operator_fn_shadows_constructor (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_operator_fn_shadows_constructor --reason "..."`

Updated:
- `expected/human-full.txt`
