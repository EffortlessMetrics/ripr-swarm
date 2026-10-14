# Golden Output Changes

## Pending — owner_return_pin_json_shadowed_owner (1)

Reason:
RIPR-SPEC-0197 #7776: audited registry-bound literal JSON admission; matched correct/wrong runtime controls preserve refusal and changed-path boundaries.

Command:
`cargo xtask goldens bless owner_return_pin_json_shadowed_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_shadowed_owner (2)

Reason:
RIPR-SPEC-0259: merged-tree golden refresh - fixture landed on main via #7828 authored before the oracle-shape Next step; the merged tree names the related test exact oracle instead of the family template. No class, stage, or evidence change.

Command:
`cargo xtask goldens bless owner_return_pin_json_shadowed_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
