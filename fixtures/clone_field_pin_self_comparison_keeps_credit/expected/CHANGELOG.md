# Golden Output Changes

## Pending — clone_field_pin_self_comparison_keeps_credit (1)

Reason:
RIPR-SPEC-0094: a related assertion comparing a value with a copy of itself is capped at weak unless the #6692 clone-field pin admits it (#13037)

Command:
`cargo xtask goldens bless clone_field_pin_self_comparison_keeps_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
