# Golden Output Changes

## Pending — rust_predicate_appended_int_arm_old_input (1)

Reason:
RIPR-SPEC-0001: an input equal to an appended integer or char equality arm reaches the change, and an appended-arm tail carries it to the return (#15027)

Command:
`cargo xtask goldens bless rust_predicate_appended_int_arm_old_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_predicate_appended_int_arm_old_input (2)

Reason:
RIPR-SPEC-0001: re-bless #15027 fixtures for the #14944 Safe next action wording

Command:
`cargo xtask goldens bless rust_predicate_appended_int_arm_old_input --reason "..."`

Updated:
- `expected/human.txt`

## Pending — rust_predicate_appended_int_arm_old_input (3)

Reason:
RIPR-SPEC-0197: #14194 withholds the false exposed return_value reading this SPEC's Known Limitation names, so the finding is weakly_exposed

Command:
`cargo xtask goldens bless rust_predicate_appended_int_arm_old_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
