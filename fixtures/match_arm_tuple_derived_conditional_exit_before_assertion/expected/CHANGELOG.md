# Golden Output Changes

## Pending — match_arm_tuple_derived_conditional_exit_before_assertion (1)

Reason:
RIPR-SPEC-0093/RIPR-SPEC-0108: #4135 new fixture pins conditional-exit non-promotion for the derived tuple arm

Command:
`cargo xtask goldens bless match_arm_tuple_derived_conditional_exit_before_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`
