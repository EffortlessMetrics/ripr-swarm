# Golden Output Changes

## Pending — typescript_snapshot_effect_receiver_redeclared (1)

Reason:
RIPR-SPEC-0258 rule 3: a redeclared receiver name is not read as the owner's class (#12465)

Command:
`cargo xtask goldens bless typescript_snapshot_effect_receiver_redeclared --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
