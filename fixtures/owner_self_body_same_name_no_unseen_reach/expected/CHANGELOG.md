# Golden Output Changes

## Pending — owner_self_body_same_name_no_unseen_reach (1)

Reason:
RIPR-SPEC-0108 #10160 owner named only in its own body has no static path

Command:
`cargo xtask goldens bless owner_self_body_same_name_no_unseen_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_self_body_same_name_no_unseen_reach (2)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless owner_self_body_same_name_no_unseen_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
