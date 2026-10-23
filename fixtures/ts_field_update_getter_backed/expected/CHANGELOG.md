# Golden Output Changes

## Pending — ts_field_update_getter_backed (1)

Reason:
RIPR-SPEC-0243 decision 20 (#9768): a field the owner's class declares as a getter or setter must not credit the field update

Command:
`cargo xtask goldens bless ts_field_update_getter_backed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_field_update_getter_backed (2)

Reason:
RIPR-SPEC-0224 merge of main into #5495: TypeScript row carries observation_unconfirmed (strong-path oracle after #9639/#9777 credit); only the miss field and its rendering change

Command:
`cargo xtask goldens bless ts_field_update_getter_backed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_field_update_getter_backed (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence legend on new current-main fixture; preserve current JSON, human-full and discriminator facts

Command:
`cargo xtask goldens bless ts_field_update_getter_backed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_field_update_getter_backed (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — ts_field_update_getter_backed (5)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless ts_field_update_getter_backed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
