# Golden Output Changes

## Pending — rust_qualified_foreign_receiver (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: qualified foreign receiver initializers cannot prove local owner reach (#8782); base/candidate and native controls retained

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
