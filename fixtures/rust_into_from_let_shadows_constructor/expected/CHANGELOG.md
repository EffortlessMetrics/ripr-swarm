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
