# Golden Output Changes

## Pending — infection_expected_value_literal (1)

Reason:
RIPR-SPEC-0001: new fixture pinning that a boundary literal counts as infection evidence only when it is an input of the changed owner (an assertion's expected value is an oracle), with a let-bound owner input as the positive control

Command:
`cargo xtask goldens bless infection_expected_value_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
