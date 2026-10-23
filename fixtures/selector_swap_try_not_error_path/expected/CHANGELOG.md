# Golden Output Changes

## Pending — selector_swap_try_not_error_path (1)

Reason:
RIPR-SPEC-0107: selector swap on a trailing-? line abstains instead of reading error_path (#9866)

Command:
`cargo xtask goldens bless selector_swap_try_not_error_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — selector_swap_try_not_error_path (2)

Reason:
RIPR-SPEC-0107: add human-full golden for the selector-swap abstention (#9866)

Command:
`cargo xtask goldens bless selector_swap_try_not_error_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
