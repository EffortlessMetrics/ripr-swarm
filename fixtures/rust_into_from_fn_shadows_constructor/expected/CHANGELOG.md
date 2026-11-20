# Golden Output Changes

## Pending — rust_into_from_fn_shadows_constructor (1)

Reason:
RIPR-SPEC-0001: a conversion or operator Rust does not dispatch to the changed impl keeps its old relation (#15277 review)

Command:
`cargo xtask goldens bless rust_into_from_fn_shadows_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
