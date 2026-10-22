# Golden Output Changes

## Pending — ts_field_update_table_zero_rows (1)

Reason:
RIPR-SPEC-0243 decision 19 (#9534): a table without a calling nonzero row must not credit the field update

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_field_update_table_zero_rows (2)

Reason:
RIPR-SPEC-0224 merge of main into #5495: TypeScript row carries observation_unconfirmed (strong-path oracle after #9639/#9777 credit); only the miss field and its rendering change

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
