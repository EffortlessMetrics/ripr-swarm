# Golden Output Changes

## Pending — type_header_bound_not_field (1)

Reason:
RIPR-SPEC-0001: a struct/enum/union/trait header with a generic bound is one static_unknown probe, not a field (#9170)

Command:
`cargo xtask goldens bless type_header_bound_not_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
