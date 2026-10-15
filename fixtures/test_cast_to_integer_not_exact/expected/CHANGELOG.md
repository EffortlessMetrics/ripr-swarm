# Golden Output Changes

## Pending — test_cast_to_integer_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1 (#8093): assert_eq!(u8::from(score(points) == 0), zero) reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless test_cast_to_integer_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
