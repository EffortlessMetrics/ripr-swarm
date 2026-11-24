# Golden Output Changes

## Pending — currentness_matrix_typescript_comment_replaced_code (1)

Reason:
RIPR-SPEC-0156: #15913 a removed guard a comment replaced keeps its base_deleted record instead of leaving none

Command:
`cargo xtask goldens bless currentness_matrix_typescript_comment_replaced_code --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
