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

## Pending — ts_field_update_table_zero_rows (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence legend on new current-main fixture; preserve current JSON, human-full and discriminator facts

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_field_update_table_zero_rows (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — ts_field_update_table_zero_rows (5)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_field_update_table_zero_rows (6)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless ts_field_update_table_zero_rows --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
