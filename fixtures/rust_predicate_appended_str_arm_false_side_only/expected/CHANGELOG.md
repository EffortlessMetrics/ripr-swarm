# Golden Output Changes

## Pending — rust_predicate_appended_str_arm_false_side_only (1)

Reason:
RIPR-SPEC-0197: a bool owner's plain string-equality tail pins on assert!(owner("lit")) like its integer twin (#15030)

Command:
`cargo xtask goldens bless rust_predicate_appended_str_arm_false_side_only --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
