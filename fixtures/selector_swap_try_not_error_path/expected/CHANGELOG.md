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

## Pending — selector_swap_try_not_error_path (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 selected Evidence legend coverage for the newly accepted fixture, preserving its current facts and all five stage values.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — selector_swap_try_not_error_path (4)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless selector_swap_try_not_error_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — selector_swap_try_not_error_path (5)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless selector_swap_try_not_error_path --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
