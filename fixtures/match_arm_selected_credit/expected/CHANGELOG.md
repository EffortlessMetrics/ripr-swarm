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
RIPR-SPEC-0002: merge of origin/main onto the #5996 workspace-relative location owner — expected files regenerate with main's match-arm verdict updates and the shared root-relative location form (issue #5996)

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
