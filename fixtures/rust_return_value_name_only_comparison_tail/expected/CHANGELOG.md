# Golden Output Changes

## Pending — rust_return_value_name_only_comparison_tail (1)

Reason:
RIPR-SPEC-0001: a name-only comparison tail and a tail if with path arms reach the return sink (#15274)

Command:
`cargo xtask goldens bless rust_return_value_name_only_comparison_tail --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_return_value_name_only_comparison_tail (2)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when State reads no test gap found

Command:
`cargo xtask goldens bless rust_return_value_name_only_comparison_tail --reason "..."`

Updated:
- `expected/human.txt`
