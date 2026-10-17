# Golden Output Changes

## Pending — field_match_foreign_option_constant (1)

Reason:
RIPR-SPEC-0229: preserve the gap when a qualified Option constant names an unselected field arm; independent runtime 1-to-999 control still passes

Command:
`cargo xtask goldens bless field_match_foreign_option_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
