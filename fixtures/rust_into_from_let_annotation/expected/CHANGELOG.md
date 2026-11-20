# Golden Output Changes

## Pending — rust_into_from_let_annotation (1)

Reason:
RIPR-SPEC-0001: an annotated .into() runs the From impl of its target type through the blanket Into (#15277)

Command:
`cargo xtask goldens bless rust_into_from_let_annotation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
