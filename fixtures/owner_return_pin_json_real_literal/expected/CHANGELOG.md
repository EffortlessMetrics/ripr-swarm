# Golden Output Changes

## Pending — owner_return_pin_json_real_literal (1)

Reason:
RIPR-SPEC-0197 #7776: audited registry-bound literal JSON admission; matched correct/wrong runtime controls preserve refusal and changed-path boundaries.

Command:
`cargo xtask goldens bless owner_return_pin_json_real_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_real_literal (2)

Reason:
RIPR-SPEC-0259: merged-tree golden refresh - fixture landed on main via #7828 authored before the oracle-shape Next step; the merged tree names the related test exact oracle instead of the family template. No class, stage, or evidence change.

Command:
`cargo xtask goldens bless owner_return_pin_json_real_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_real_literal (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless owner_return_pin_json_real_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
