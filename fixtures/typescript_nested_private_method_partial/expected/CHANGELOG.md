# Golden Output Changes

## Pending — typescript_nested_private_method_partial (1)

Reason:
RIPR-SPEC-0243: nested private changes must remain partial and cannot borrow public reach

Command:
`cargo xtask goldens bless typescript_nested_private_method_partial --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
