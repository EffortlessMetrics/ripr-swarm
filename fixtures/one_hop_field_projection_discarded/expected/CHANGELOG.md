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
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless one_hop_field_projection_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — one_hop_field_projection_discarded (3)

Reason:
RIPR-SPEC-0122: re-bless for #8412 stub route/refusal now printed for every probe family (call_deletion, field_construction, side_effect); drift is solely the inserted stub-route line, no classification or count change

Command:
`cargo xtask goldens bless one_hop_field_projection_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
