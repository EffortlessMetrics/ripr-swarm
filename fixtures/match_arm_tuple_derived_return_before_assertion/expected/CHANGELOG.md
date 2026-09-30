# Golden Output Changes

## Pending — match_arm_tuple_derived_return_before_assertion (1)

Reason:
RIPR-SPEC-0093/RIPR-SPEC-0108: #4068 review — new fixture pins the derived tuple-arm return-boundary non-promotion (relation assertion after a direct return stays weakly_exposed)

Command:
`cargo xtask goldens bless match_arm_tuple_derived_return_before_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_derived_return_before_assertion (2)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless match_arm_tuple_derived_return_before_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
