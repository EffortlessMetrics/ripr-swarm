# Golden Output Changes

## Pending — member_reexport_foreign_trait_keeps_dispatch_allowance (1)

Reason:
RIPR-SPEC-0108: new control, a member re-export of a foreign trait keeps the #9342 dispatch allowance (#10061)

Command:
`cargo xtask goldens bless member_reexport_foreign_trait_keeps_dispatch_allowance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — member_reexport_foreign_trait_keeps_dispatch_allowance (2)

Reason:
RIPR-SPEC-0108: make the re-export control token-clean so the trait rule alone decides it (no fmt identifier outside the fn line); confirmed by mutation (#10061)

Command:
`cargo xtask goldens bless member_reexport_foreign_trait_keeps_dispatch_allowance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
