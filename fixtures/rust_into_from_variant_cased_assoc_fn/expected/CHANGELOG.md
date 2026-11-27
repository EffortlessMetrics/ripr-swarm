# Golden Output Changes

## Pending — rust_into_from_variant_cased_assoc_fn (1)

Reason:
RIPR-SPEC-0001: a conversion the .into() reading cannot pin to the changed impl keeps its old relation (#15277 review)

Command:
`cargo xtask goldens bless rust_into_from_variant_cased_assoc_fn --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_variant_cased_assoc_fn (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_into_from_variant_cased_assoc_fn --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_variant_cased_assoc_fn (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless rust_into_from_variant_cased_assoc_fn --reason "..."`

Updated:
- `expected/human-full.txt`
