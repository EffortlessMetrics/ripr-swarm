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

## Pending — rust_trait_qualified_associated_call (2)

Reason:
RIPR-SPEC-0108: composed with the import line's delta-token confirmation rule, the surviving token Square no longer confirms a plain return and no owner-return pin reads a trait-impl associated function without self; the relation stays direct_owner_call (#15277) and the class is a recorded under-credit

Command:
`cargo xtask goldens bless rust_trait_qualified_associated_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
