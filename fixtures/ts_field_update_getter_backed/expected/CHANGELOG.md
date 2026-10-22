# Golden Output Changes

## Pending — ts_field_update_getter_backed (1)

Reason:
RIPR-SPEC-0243 decision 20 (#9768): a field the owner's class declares as a getter or setter must not credit the field update

Command:
`cargo xtask goldens bless ts_field_update_getter_backed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
