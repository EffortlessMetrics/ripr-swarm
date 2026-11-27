# Golden Output Changes

## Pending — rust_into_from_let_shadows_constructor (1)

Reason:
RIPR-SPEC-0001: a let binding named like a braced source struct shadows its call spelling, so .into() keeps its old relation (#15277 review)

Command:
`cargo xtask goldens bless rust_into_from_let_shadows_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_into_from_let_shadows_constructor (2)

Reason:
RIPR-SPEC-0001: refresh human summary wording after merging main; no classification change (#15277)

Command:
`cargo xtask goldens bless rust_into_from_let_shadows_constructor --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_into_from_let_shadows_constructor (3)

Reason:
RIPR-SPEC-0108: compose PR #16016 history-sync with protected swarm main (baaaa92024)

Command:
`cargo xtask goldens bless rust_into_from_let_shadows_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
