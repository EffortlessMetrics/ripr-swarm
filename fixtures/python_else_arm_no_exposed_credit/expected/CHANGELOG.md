# Golden Output Changes

## Pending — python_else_arm_no_exposed_credit (1)

Reason:
pin RIPR-SPEC-0260 else-arm evidence for #9184: every strong call takes the earlier arm

Command:
`cargo xtask goldens bless python_else_arm_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_else_arm_no_exposed_credit (2)

Reason:
RIPR-SPEC-0260: add the human-full golden for the #9184 else-arm fixture

Command:
`cargo xtask goldens bless python_else_arm_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_else_arm_no_exposed_credit (3)

Reason:
RIPR-SPEC-0122: explain the selected bounded Evidence stages for #9373 item 1; retain the accepted RIPR-SPEC-0260 else-arm evidence and all JSON/full facts.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line in `expected/human.txt`.

Execution:
Source-only literal/slot comparison; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — python_else_arm_no_exposed_credit (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — python_else_arm_no_exposed_credit (5)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless python_else_arm_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
