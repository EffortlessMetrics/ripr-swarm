# Golden Output Changes

## Pending — typescript_typeof_negation_other_kind_exposed (1)

Reason:
RIPR-SPEC-0027: a negated typeof guard is witnessed by an input of any written kind; a changed kind name keeps the exact-kind rule (#12281)

Command:
`cargo xtask goldens bless typescript_typeof_negation_other_kind_exposed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
