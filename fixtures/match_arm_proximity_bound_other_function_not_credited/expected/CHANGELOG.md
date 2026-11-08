# Golden Output Changes

## Pending — match_arm_proximity_bound_other_function_not_credited (1)

Reason:
RIPR-SPEC-0094: new fixture for #12056; a file-proximity test whose variant assertion reads a local bound to an indexed function with no path to the owner cannot confirm the arm

Command:
`cargo xtask goldens bless match_arm_proximity_bound_other_function_not_credited --reason "..."`

Updated:
- `expected/check.json`
