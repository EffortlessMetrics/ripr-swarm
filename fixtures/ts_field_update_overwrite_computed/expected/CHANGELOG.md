# Golden Output Changes

## Pending — ts_field_update_overwrite_computed (1)

Reason:
RIPR-SPEC-0243 decision 18 (#9515): a write the field-update scan cannot see through the receiver must refuse credit

Command:
`cargo xtask goldens bless ts_field_update_overwrite_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
