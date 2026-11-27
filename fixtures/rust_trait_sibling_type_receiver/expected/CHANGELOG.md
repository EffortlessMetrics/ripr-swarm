# Golden Output Changes

## Pending — rust_trait_sibling_type_receiver (1)

Reason:
RIPR-SPEC-0108: a sibling impl on another named type does not defeat the owner's return pin (#15276)

Command:
`cargo xtask goldens bless rust_trait_sibling_type_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
