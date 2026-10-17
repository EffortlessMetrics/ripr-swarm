# Golden Output Changes

## Pending — field_match_raw_option_shadow (1)

Reason:
RIPR-SPEC-0229: raw option constants in distinct scopes cannot borrow the prelude selector identity

Command:
`cargo xtask goldens bless field_match_raw_option_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
