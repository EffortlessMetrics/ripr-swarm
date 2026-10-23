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

## Pending — ts_field_update_overwrite_alias (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence stage and state legend; preserve current analysis, JSON and human-full

Command:
`cargo xtask goldens bless ts_field_update_overwrite_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — ts_field_update_overwrite_alias (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
