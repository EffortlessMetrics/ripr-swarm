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
RIPR-SPEC-0045: combined-tree re-bless after merging #5432/#5638 - the Rust canonical gap members (#5268) join the match-arm verdict re-bless on these fixtures; golden-drift.json shows zero semantic flips

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (3)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (4)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (5)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (6)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (7)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (8)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (10)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — match_arm_selected_credit (11)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — match_arm_selected_credit (12)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
