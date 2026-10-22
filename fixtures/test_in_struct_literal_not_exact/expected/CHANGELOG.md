# Golden Output Changes

## Pending — test_in_struct_literal_not_exact (1)

Reason:
RIPR-SPEC-0231: a test held in a struct literal compared with a non-literal reads weak, not exposed (#8615)

Command:
`cargo xtask goldens bless test_in_struct_literal_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — test_in_struct_literal_not_exact (2)

Reason:
RIPR-SPEC-0231: human output on main now offers no repair start for the weakly exposed finding; refresh after merging main

Command:
`cargo xtask goldens bless test_in_struct_literal_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
