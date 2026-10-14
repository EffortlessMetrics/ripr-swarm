# Golden Output Changes

## Pending — match_arm_tuple_terminal_imported_formatter (1)

Reason:
RIPR-SPEC-0093: retain the compiling imported-formatter counterexample for ripr issue 1728; exact terminal match stays observation_unverified because the formatter ignores the relation.

Command:
`cargo xtask goldens bless match_arm_tuple_terminal_imported_formatter --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
