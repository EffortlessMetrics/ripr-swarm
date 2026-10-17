# Golden Output Changes

## Pending — owner_return_pin_json_ignored_result (1)

Reason:
RIPR-SPEC-0197 #7776: audited registry-bound literal JSON admission; matched correct/wrong runtime controls preserve refusal and changed-path boundaries.

Command:
`cargo xtask goldens bless owner_return_pin_json_ignored_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_ignored_result (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless owner_return_pin_json_ignored_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_ignored_result (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless owner_return_pin_json_ignored_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_json_ignored_result (4)

Reason:
RIPR-SPEC-0122: #8389 Summary names base-side findings, Hidden total equals listed bullets, all-unknown footer no longer claims a found absence

Command:
`cargo xtask goldens bless owner_return_pin_json_ignored_result --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
