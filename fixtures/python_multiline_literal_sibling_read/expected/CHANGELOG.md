# Golden Output Changes

## Pending — python_multiline_literal_sibling_read (1)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: pin a multi-line literal element observed only through an unchanged sibling, and a changed number found only inside a longer number, as weakly_exposed (#10210)

Command:
`cargo xtask goldens bless python_multiline_literal_sibling_read --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
