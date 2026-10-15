# Golden Output Changes

## Pending — test_compared_with_non_literal_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1 (#8006): assert_eq!(score(points) == 0, expected) reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless test_compared_with_non_literal_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
