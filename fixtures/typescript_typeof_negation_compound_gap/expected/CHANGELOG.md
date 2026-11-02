# Golden Output Changes

## Pending — typescript_typeof_negation_compound_gap (1)

Reason:
RIPR-SPEC-0027: a negated typeof clause joined by && keeps the exact-kind rule and reads weak (#12281 review)

Command:
`cargo xtask goldens bless typescript_typeof_negation_compound_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
