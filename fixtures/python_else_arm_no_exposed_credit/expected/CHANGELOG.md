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
