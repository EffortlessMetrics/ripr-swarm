# Golden Output Changes

## Pending — match_arm_tuple_terminal_generated_constructor (1)

Reason:
RIPR-SPEC-0093: new macro-generated inherited constructor refusal witness.
Expected outputs retain the written-constructor fixture's unverified contract;
no existing golden was changed or re-blessed.

## Pending — match_arm_tuple_terminal_generated_constructor (2)

Reason:
RIPR-SPEC-0093: re-bless human output against current plain-language renderer (#8165)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_generated_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_generated_constructor (3)

Reason:
RIPR-SPEC-0093: re-bless human summary line with the base-side evidence suffix (stale after #7836 merged)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_generated_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
