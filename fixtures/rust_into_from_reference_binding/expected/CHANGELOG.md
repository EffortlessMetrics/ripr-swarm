# Golden Output Changes

## Pending — rust_into_from_reference_binding (1)

Reason:
RIPR-SPEC-0001: a .into() that Rust does not dispatch to the changed From impl keeps its file-only relation (#15277)

Command:
`cargo xtask goldens bless rust_into_from_reference_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_reference_binding (2)

Reason:
RIPR-SPEC-0001: refresh human summary wording after merging main; no classification change (#15277)

Command:
`cargo xtask goldens bless rust_into_from_reference_binding --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_into_from_reference_binding (3)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless rust_into_from_reference_binding --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`
