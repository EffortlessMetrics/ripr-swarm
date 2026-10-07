
## Pending — owner_name_collision_helper_keeps_credit (1)

Reason:
RIPR-SPEC-0094: name-collision control stays exposed (#5830 review A1)

Command:
`cargo xtask goldens bless owner_name_collision_helper_keeps_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_name_collision_helper_keeps_credit (2)

Reason:
RIPR-SPEC-0094 #5830 (ported from #6624 onto current main): owner-scoped tokens confirm only in assertions bound to the owner; self-computed expected values are weak and unconfirmed. Output re-rendered in main's current format.

Command:
`cargo xtask goldens bless owner_name_collision_helper_keeps_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
