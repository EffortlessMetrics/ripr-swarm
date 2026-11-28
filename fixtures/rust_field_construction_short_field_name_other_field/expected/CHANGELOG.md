# Golden Output Changes

## Pending — rust_field_construction_short_field_name_other_field (1)

Reason:
RIPR-SPEC-0001: a read of another field keeps a short changed field unconfirmed (#15274)

Command:
`cargo xtask goldens bless rust_field_construction_short_field_name_other_field --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
