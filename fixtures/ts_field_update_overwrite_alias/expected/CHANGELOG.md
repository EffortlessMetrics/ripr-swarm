# Golden Output Changes

## Pending — ts_field_update_overwrite_alias (1)

Reason:
RIPR-SPEC-0243 decision 18 (#9515): a write the field-update scan cannot see through the receiver must refuse credit

Command:
`cargo xtask goldens bless ts_field_update_overwrite_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_field_update_overwrite_alias (2)

Reason:
RIPR-SPEC-0224: TypeScript row-local miss on main's new fixture (#5495)

Command:
`cargo xtask goldens bless ts_field_update_overwrite_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
