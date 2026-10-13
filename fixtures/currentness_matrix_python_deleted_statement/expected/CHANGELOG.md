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
RIPR-SPEC-0156: combined-tree drift on main (#7680 merged beside #7577, which added the loaded config path and next action to check output); no verdict change, same re-bless as #7700

Command:
`cargo xtask goldens bless currentness_matrix_python_deleted_statement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
