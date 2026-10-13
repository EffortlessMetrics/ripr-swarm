# Golden Output Changes

## Pending — currentness_matrix_python_deleted_statement (1)

Reason:
RIPR-SPEC-0156: #7587 keeps an unpaired deleted Python behavior line as base_deleted evidence on its owner

Command:
`cargo xtask goldens bless currentness_matrix_python_deleted_statement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — currentness_matrix_python_deleted_statement (2)

Reason:
RIPR-SPEC-0156: #7587 review: base-side lines are not counted as candidate lines

Command:
`cargo xtask goldens bless currentness_matrix_python_deleted_statement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — currentness_matrix_python_deleted_statement (3)

Reason:
RIPR-SPEC-0255: align the #7680 fixture blessing (made on a stale base) with the check config-path disclosure that #7577 landed on main; bare main 728e3427c fails this same goldens check (run 38408527126)

Command:
`cargo xtask goldens bless currentness_matrix_python_deleted_statement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
