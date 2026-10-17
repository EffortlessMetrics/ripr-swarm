# Golden Output Changes

## Pending — one_hop_field_projection_discarded (1)

Reason:
RIPR-SPEC-0159 exact one-hop field observation; retained runtime count controls distinguish wrong sink, discarded result and sibling field.

Command:
`cargo xtask goldens bless one_hop_field_projection_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — one_hop_field_projection_discarded (2)

Reason:
RIPR-SPEC-0122: plain-language bounded rendering already landed on main (#8165); stale golden refresh

Command:
`cargo xtask goldens bless one_hop_field_projection_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
