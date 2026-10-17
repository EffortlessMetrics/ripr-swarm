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
RIPR-SPEC-0122: human check output prints the stub route or a No test stub here refusal for every probe family (#8412)

RIPR-SPEC-0158 and #7805: additive source_file metadata on test-source values only; check.json lines otherwise byte-identical

Command:
`cargo xtask goldens bless one_hop_field_projection_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
