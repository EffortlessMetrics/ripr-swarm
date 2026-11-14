# Golden Output Changes

## Pending — match_arm_proximity_other_function_not_credited (1)

Reason:
RIPR-SPEC-0094: new fixture for #11552; a file-proximity test whose variant assertion reads an indexed function with no path to the owner cannot confirm the arm

Command:
`cargo xtask goldens bless match_arm_proximity_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_proximity_other_function_not_credited (2)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless match_arm_proximity_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`

## Pending — match_arm_proximity_other_function_not_credited (3)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless match_arm_proximity_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`

## Pending — match_arm_proximity_other_function_not_credited (4)

Reason:
RIPR-SPEC-0001: head-only match-arm rows and their propagation sink name the arm's same-line result (#14420)

Command:
`cargo xtask goldens bless match_arm_proximity_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
