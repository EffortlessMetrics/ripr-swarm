# Golden Output Changes

## Pending — typescript_snapshot_effect_unrelated_committed (1)

Reason:
RIPR-SPEC-0258 rule 3: committed snapshot of an unrelated subject no longer confirms the effect seam (#12465)

Command:
`cargo xtask goldens bless typescript_snapshot_effect_unrelated_committed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
