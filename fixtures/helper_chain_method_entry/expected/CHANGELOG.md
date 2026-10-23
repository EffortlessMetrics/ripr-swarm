# Golden Output Changes

## Pending — helper_chain_method_entry (1)

Reason:
RIPR-SPEC-0159: method-hop entry pin credits private method through public entry (#5352)

Command:
`cargo xtask goldens bless helper_chain_method_entry --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — helper_chain_method_entry (2)

Reason:
RIPR-SPEC-0159: main added the no-repair-start line to human output; formatting only

Command:
`cargo xtask goldens bless helper_chain_method_entry --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — helper_chain_method_entry (3)

Reason:
RIPR-SPEC-0159: formatting-only, main's no-repair-start line now names missing evidence

Command:
`cargo xtask goldens bless helper_chain_method_entry --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — helper_chain_method_entry (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 selected Evidence legend coverage for the newly accepted method-entry fixture, preserving its current facts and all five stage values.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — helper_chain_method_entry (5)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless helper_chain_method_entry --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
