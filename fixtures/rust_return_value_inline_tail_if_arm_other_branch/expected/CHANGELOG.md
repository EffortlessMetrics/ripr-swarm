# Golden Output Changes

## Pending — rust_return_value_inline_tail_if_arm_other_branch (1)

Reason:
RIPR-SPEC-0001: a one-line tail if arm edit probes the changed arm (#15274)

Command:
`cargo xtask goldens bless rust_return_value_inline_tail_if_arm_other_branch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
