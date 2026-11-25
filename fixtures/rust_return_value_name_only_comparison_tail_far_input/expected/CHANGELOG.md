# Golden Output Changes

## Pending — rust_return_value_name_only_comparison_tail_far_input (1)

Reason:
RIPR-SPEC-0001: a name-only comparison tail and a tail if with path arms reach the return sink (#15274)

Command:
`cargo xtask goldens bless rust_return_value_name_only_comparison_tail_far_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
