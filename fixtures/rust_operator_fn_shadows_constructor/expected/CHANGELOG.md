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
RIPR-SPEC-0108: compose PR #16016 history-sync with protected swarm main (baaaa92024)

Command:
`cargo xtask goldens bless rust_operator_fn_shadows_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
