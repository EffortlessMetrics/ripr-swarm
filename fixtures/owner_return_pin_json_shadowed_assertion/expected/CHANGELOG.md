# Golden Output Changes

## Pending — owner_return_pin_json_shadowed_assertion (1)

Reason:
RIPR-SPEC-0197 #7776: audited registry-bound literal JSON admission; matched correct/wrong runtime controls preserve refusal and changed-path boundaries.

Command:
`cargo xtask goldens bless owner_return_pin_json_shadowed_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
