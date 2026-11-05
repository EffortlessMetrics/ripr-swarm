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
RIPR-SPEC-0094 / RIPR-SPEC-0197: retain fixture classifications while refreshing composed producer guidance and assertion-local sink identity; owner-call proximity alone does not establish discriminator credit

Command:
`cargo xtask goldens bless match_arm_proximity_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
