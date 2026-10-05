# Golden Output Changes

## Pending — inequality_assertion_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1: assert_ne! reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
