# Golden Output Changes

## Pending — rust_inline_operator_branch_not_taken (1)

Reason:
RIPR-SPEC-0108: operator reach alone cannot expose a return in the untaken branch (#15529)

Command:
`cargo xtask goldens bless rust_inline_operator_branch_not_taken --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
