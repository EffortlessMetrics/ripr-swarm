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

## Pending — ts_multi_value_field_change_sibling (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless ts_multi_value_field_change_sibling --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`
