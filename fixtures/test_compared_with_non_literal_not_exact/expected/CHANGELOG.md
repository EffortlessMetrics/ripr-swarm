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

## Pending — test_compared_with_non_literal_not_exact (2)

Reason:
RIPR-SPEC-0002: #7755 double-colon fix (language counts use =); #7756 labelled human lines carry no terminal period

Command:
`cargo xtask goldens bless test_compared_with_non_literal_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
