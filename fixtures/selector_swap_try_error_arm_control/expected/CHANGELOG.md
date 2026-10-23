# Golden Output Changes

## Pending — selector_swap_try_error_arm_control (1)

Reason:
RIPR-SPEC-0107: positive control, a changed ? error variant stays error_path (#9866)

Command:
`cargo xtask goldens bless selector_swap_try_error_arm_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — selector_swap_try_error_arm_control (2)

Reason:
RIPR-SPEC-0107: add human-full golden for the changed-error-variant control (#9866)

Command:
`cargo xtask goldens bless selector_swap_try_error_arm_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — selector_swap_try_error_arm_control (3)

Reason:
RIPR-SPEC-0122: #9928 dropped the (observation_unverified) token from grouped human evidence rows and re-blessed 34 human-full goldens; #9950's selector_swap_try_error_arm_control human-full golden merged independently with the old text, so re-bless to remove the token (wording-only, formatting_only)

Command:
`cargo xtask goldens bless selector_swap_try_error_arm_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
