# Golden Output Changes

## Pending — typescript_deep_nesting_parse_budget (1)

Reason:
RIPR-SPEC-0027: new fixture for #4101, 500-deep TS nesting discloses a parse-budget limitation instead of aborting

Command:
`cargo xtask goldens bless typescript_deep_nesting_parse_budget --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_deep_nesting_parse_budget (2)

Reason:
RIPR-SPEC-0027: re-bless #4101 fixture after budget-specific recovery wording

Command:
`cargo xtask goldens bless typescript_deep_nesting_parse_budget --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
