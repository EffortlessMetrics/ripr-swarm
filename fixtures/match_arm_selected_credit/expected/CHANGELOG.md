# Golden Output Changes

## Pending — match_arm_selected_credit (1)

Reason:
RIPR-SPEC-0093: new fixture (#5432) - an owner call passing None selects the None arm inside an exact assertion, so the arm reads exposed

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (2)

Reason:
RIPR-SPEC-0045: combined-tree re-bless after merging #5432/#5638 - the Rust canonical gap members (#5268) join the match-arm verdict re-bless on these fixtures; golden-drift.json shows zero semantic flips

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
