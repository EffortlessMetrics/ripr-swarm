# Golden Output Changes

## Pending — ts_multi_value_field_change_nonnull_negation (1)

Reason:
RIPR-SPEC-0243: new fixture pinning the #14928 non-null and negation untouched-field downgrade

Command:
`cargo xtask goldens bless ts_multi_value_field_change_nonnull_negation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`

## Pending — ts_multi_value_field_change_nonnull_negation (2)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless ts_multi_value_field_change_nonnull_negation --reason "..."`

Updated:
- `expected/human.txt`
- `expected/human-full.txt`
