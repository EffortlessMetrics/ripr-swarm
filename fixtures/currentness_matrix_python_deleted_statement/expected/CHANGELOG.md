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
RIPR-SPEC-0255: #7577 names the loaded ripr.toml in the human header and JSON config_path; re-bless the fixture #7680 added before #7577 merged

Command:
`cargo xtask goldens bless currentness_matrix_python_deleted_statement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
