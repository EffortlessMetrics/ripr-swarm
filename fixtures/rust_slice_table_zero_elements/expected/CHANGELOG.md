# Golden Output Changes

## Pending — rust_slice_table_zero_elements (1)

Reason:
RIPR-SPEC-0001: Slice elements 0 do not credit length 0; independent controls establish cardinalities 2 and 3 (#7553).

Command:
`cargo xtask goldens bless rust_slice_table_zero_elements --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_zero_elements (2)

Reason:
RIPR-SPEC-0158 and #7805: independently audited additive source_file metadata only; all classification, oracle, evidence and human output preserved

Command:
`cargo xtask goldens bless rust_slice_table_zero_elements --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
