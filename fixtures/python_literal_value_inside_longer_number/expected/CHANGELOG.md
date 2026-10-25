# Golden Output Changes

## Pending — python_literal_value_inside_longer_number (1)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: pin a multi-line literal element observed only through an unchanged sibling, and a changed number found only inside a longer number, as weakly_exposed (#10210)

Command:
`cargo xtask goldens bless python_literal_value_inside_longer_number --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_literal_value_inside_longer_number (2)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless python_literal_value_inside_longer_number --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
