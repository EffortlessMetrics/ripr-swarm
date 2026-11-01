# Golden Output Changes

## Pending — invisible_change_relational_sibling (1)

Reason:
RIPR-SPEC-0197: #10916 relational sibling downgrade charter fixture; initial golden pins weakly_exposed with the truth-value infection summary

Command:
`cargo xtask goldens bless invisible_change_relational_sibling --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — invisible_change_relational_sibling (2)

Reason:
RIPR-SPEC-0122: #10920 review - the weak-infection hint claimed no test input tells the versions apart; the admitted relational sibling case (#10916) has asserted inputs that evaluate differently, so the hint now states the assertion-level fact

Command:
`cargo xtask goldens bless invisible_change_relational_sibling --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — invisible_change_relational_sibling (3)

Reason:
RIPR-SPEC-0122 plain wording for the check screen's whole-seam/untied-spot refusals (#11619)

Command:
`cargo xtask goldens bless invisible_change_relational_sibling --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
