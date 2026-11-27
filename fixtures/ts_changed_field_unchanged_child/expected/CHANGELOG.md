# Golden Output Changes

## Pending — ts_changed_field_unchanged_child (1)

Reason:
RIPR-SPEC-0243: new fixture pinning the #14858 unchanged-child downgrade

Command:
`cargo xtask goldens bless ts_changed_field_unchanged_child --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`

## Pending — ts_changed_field_unchanged_child (2)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless ts_changed_field_unchanged_child --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`
