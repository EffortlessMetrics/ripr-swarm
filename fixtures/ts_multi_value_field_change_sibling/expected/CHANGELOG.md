# Golden Output Changes

## Pending — ts_multi_value_field_change_sibling (1)

Reason:
RIPR-SPEC-0243: new fixture pinning the #14857 two-value field-change downgrade

Command:
`cargo xtask goldens bless ts_multi_value_field_change_sibling --reason "..."`

Updated:
- `expected/check.json`

## Pending — ts_multi_value_field_change_sibling (2)

Reason:
human-full: one blank line, not two, separates the last finding from a trailing Note (#14318)

Command:
`cargo xtask goldens bless ts_multi_value_field_change_sibling --reason "..."`

Updated:
- `expected/human-full.txt`
