# Golden Output Changes

## Pending — rust_predicate_appended_str_arm_truth_assert (1)

Reason:
RIPR-SPEC-0197: a bool owner's plain string-equality tail pins on assert!(owner("lit")) like its integer twin (#15030)

Command:
`cargo xtask goldens bless rust_predicate_appended_str_arm_truth_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_predicate_appended_str_arm_truth_assert (2)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when State reads no test gap found

Command:
`cargo xtask goldens bless rust_predicate_appended_str_arm_truth_assert --reason "..."`

Updated:
- `expected/human.txt`
