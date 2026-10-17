# Golden Output Changes

## Pending — match_arm_tuple_terminal_inherited_constructor (1)

Reason:
RIPR-SPEC-0093: retain the compiling crate-root Some constructor counterexample; the relation stays observation_unverified.

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (2)

Reason:
RIPR-SPEC-0093: re-bless human output against current plain-language renderer (#8165)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_inherited_constructor (3)

Reason:
RIPR-SPEC-0122: add the base-side evidence count to the Summary line (#8493); wording only, no class change

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_inherited_constructor --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
