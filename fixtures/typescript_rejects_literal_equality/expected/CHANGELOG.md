# Golden Output Changes

## Pending — typescript_rejects_literal_equality (1)

Reason:
RIPR-SPEC-0243 / #7315: independent runtime proof pins rejection equality to error paths and withholds normal-return credit

Command:
`cargo xtask goldens bless typescript_rejects_literal_equality --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_rejects_literal_equality (2)

Reason:
RIPR-SPEC-0243 / #7315: retain full human output for independent error and return domain assertions

Command:
`cargo xtask goldens bless typescript_rejects_literal_equality --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
