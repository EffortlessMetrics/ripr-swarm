# Golden Output Changes

## Pending — rust_predicate_appended_char_arm_old_input (1)

Reason:
RIPR-SPEC-0001: a char-literal appended-arm tail reaches the return sink like its integer twin (#15029)

Command:
`cargo xtask goldens bless rust_predicate_appended_char_arm_old_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_predicate_appended_char_arm_old_input (2)

Reason:
RIPR-SPEC-0259: a -> bool owner whose reaching related tests already assert its exact result with assert!(owner(..)) / assert!(!owner(..)) gets a next step naming the missing deciding input instead of the broad-assertion template (#15149); classification, stages and evidence unchanged

Command:
`cargo xtask goldens bless rust_predicate_appended_char_arm_old_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
