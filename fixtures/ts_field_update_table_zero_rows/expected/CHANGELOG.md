# Golden Output Changes

## Pending — ts_field_update_table_zero_rows (1)

Reason:
RIPR-SPEC-0243 decision 19 (#9534): a table without a calling nonzero row must not credit the field update

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
