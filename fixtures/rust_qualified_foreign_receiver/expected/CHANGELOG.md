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

## Pending — rust_qualified_foreign_receiver (2)

Reason:
RIPR-SPEC-0005: gap withholding on main reports the unresolved foreign-owner receivers as static_unknown (gap_evidence_unresolved) instead of weakly_exposed; still no promotion to exposed (#8782 item 1)

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_foreign_receiver (3)

Reason:
RIPR-SPEC-0005: confidence on the static_unknown finding moved 0.66 to 0.56 with current main; class, stop reason and relation rows unchanged (#8782 item 1)

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_foreign_receiver (4)

Reason:
RIPR-SPEC-0319: main #9449 added Next guidance lines under static limitations without reblessing these fixtures (inherited red goldens); human.txt also carries this PR's RIPR-SPEC-0122 clipped-to-wrap line

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_foreign_receiver (5)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_foreign_receiver (6)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless rust_qualified_foreign_receiver --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
