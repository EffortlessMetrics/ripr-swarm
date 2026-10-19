# Golden Output Changes

## Pending — match_arm_tuple_terminal_imported_formatter (1)

Reason:
RIPR-SPEC-0093: retain the compiling imported-formatter counterexample for ripr issue 1728; exact terminal match stays observation_unverified because the formatter ignores the relation.

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_tuple_terminal_imported_formatter (2)

Reason:
RIPR-SPEC-0093: re-bless human output against current plain-language renderer (#8165)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_imported_formatter (3)

Reason:
RIPR-SPEC-0093: re-bless human summary line with the base-side evidence suffix (stale after #7836 merged)

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_imported_formatter (4)

Reason:
RIPR-SPEC-0108: #7840 wrapped-signature parameter names now label observed function-argument values; classification unchanged

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — match_arm_tuple_terminal_imported_formatter (5)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
