# Golden Output Changes

## Pending — python_message_pin_no_exposed_credit (1)

Reason:
RIPR-SPEC-0238: pin the #9185 message-only raise fixture where a literal match pattern fits both messages

Command:
`cargo xtask goldens bless python_message_pin_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_message_pin_no_exposed_credit (2)

Reason:
RIPR-SPEC-0122: #9373 item 1 selected Evidence legend coverage for the newly accepted fixture, preserving its current facts and all five stage values.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — python_message_pin_no_exposed_credit (3)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless python_message_pin_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
