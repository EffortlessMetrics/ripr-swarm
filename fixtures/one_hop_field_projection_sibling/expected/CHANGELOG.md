# Golden Output Changes

## Pending — one_hop_field_projection_sibling (1)

Reason:
RIPR-SPEC-0159 exact one-hop field observation; retained runtime count controls distinguish wrong sink, discarded result and sibling field.

Command:
`cargo xtask goldens bless one_hop_field_projection_sibling --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — one_hop_field_projection_sibling (2)

Reason:
RIPR-SPEC-0002: re-bless the plain-language Summary line (#8165) that landed without refreshing this fixture; formatting-only, ported from #8351

Command:
`cargo xtask goldens bless one_hop_field_projection_sibling --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
