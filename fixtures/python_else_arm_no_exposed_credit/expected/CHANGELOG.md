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
