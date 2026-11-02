# Golden Output Changes

## Pending — match_arm_tuple_derived_conditional_exit_before_assertion (1)

Reason:
RIPR-SPEC-0093/RIPR-SPEC-0108: #4135 new fixture pins conditional-exit non-promotion for the derived tuple arm

Command:
`cargo xtask goldens bless match_arm_tuple_derived_conditional_exit_before_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`

## Pending — match_arm_tuple_derived_conditional_exit_before_assertion (2)

Reason:
RIPR-SPEC-0152: base-side findings (base_deleted, moved_or_renamed) carry the not-a-candidate-edit-target refusal as their next step instead of test advice (#11905)

Command:
`cargo xtask goldens bless match_arm_tuple_derived_conditional_exit_before_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
