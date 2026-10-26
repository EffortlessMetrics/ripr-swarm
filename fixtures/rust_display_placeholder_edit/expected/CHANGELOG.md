# Golden Output Changes

## Pending — rust_display_placeholder_edit (1)

Reason:
RIPR-SPEC-0297 #9836: encoded placeholder changes cannot borrow an exact Display pin

Command:
`cargo xtask goldens bless rust_display_placeholder_edit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
