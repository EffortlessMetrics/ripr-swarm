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
RIPR-SPEC-0158 and #7805: independently audited additive source_file metadata only; all classification, oracle, evidence and human output preserved

Command:
`cargo xtask goldens bless match_arm_selected_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
