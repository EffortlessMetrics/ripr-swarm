# Golden Output Changes

## Pending — rust_trait_qualified_associated_call (1)

Reason:
RIPR-SPEC-0108: a fully qualified call of a trait function without self names its impl (#15277)

Command:
`cargo xtask goldens bless rust_trait_qualified_associated_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
