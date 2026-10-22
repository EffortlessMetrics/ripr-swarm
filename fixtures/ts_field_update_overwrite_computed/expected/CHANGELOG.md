# Golden Output Changes

## Pending — ts_field_update_overwrite_computed (1)

Reason:
RIPR-SPEC-0243 decision 18 (#9515): a write the field-update scan cannot see through the receiver must refuse credit

Command:
`cargo xtask goldens bless ts_field_update_overwrite_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_field_update_overwrite_computed (2)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence stage and state legend; preserve current analysis, JSON and human-full

Command:
`cargo xtask goldens bless ts_field_update_overwrite_computed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
