# Golden Output Changes

## Pending — bool_compared_inequality_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1 (#7598): assert_eq!(score(2) != 0, true) reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless bool_compared_inequality_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
