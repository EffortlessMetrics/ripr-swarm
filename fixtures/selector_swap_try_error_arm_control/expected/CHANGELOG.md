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
RIPR-SPEC-0122: #9921 dropped the observation_unverified suffix from per-finding human evidence lines and re-blessed 57 human-full goldens, missing selector_swap_try_error_arm_control which #9950 blessed in the same batch; formatting_only drift, one line, mechanical re-bless to the current renderer.

Command:
`cargo xtask goldens bless selector_swap_try_error_arm_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
