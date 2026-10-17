# Golden Output Changes

## Pending — python_cross_test_import_alias (1)

Reason:
RIPR-SPEC-0028: new leaf fixture for #8247 cross-test import alias; alias binds only in the importing test, so the finding is not exposed

Command:
`cargo xtask goldens bless python_cross_test_import_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_cross_test_import_alias (2)

Reason:
RIPR-SPEC-0028: add required human-full projection for the independent RIPR-SPEC-0108 honesty case

Command:
`cargo xtask goldens bless python_cross_test_import_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
