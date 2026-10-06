# Golden Output Changes

## Pending — gap_withheld_feature_gated_test (1)

Reason:
RIPR-SPEC-0240: paired fixture for gap withholding on analyzer-limit assertion refusals

Command:
`cargo xtask goldens bless gap_withheld_feature_gated_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_withheld_feature_gated_test (2)

Reason:
RIPR-SPEC-0240: related tests carry #5424 per-test miss reasons

Command:
`cargo xtask goldens bless gap_withheld_feature_gated_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_withheld_feature_gated_test (3)

Reason:
RIPR-SPEC-0240: rebased onto main, where finding paths render workspace-relative and gaps carry canonical_gap_id

Command:
`cargo xtask goldens bless gap_withheld_feature_gated_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — gap_withheld_feature_gated_test (4)

Reason:
RIPR-SPEC-0240: a withheld gap claims no missing test, so it gets no test-writing route, and its next step no longer points at the not-credited note

Command:
`cargo xtask goldens bless gap_withheld_feature_gated_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
