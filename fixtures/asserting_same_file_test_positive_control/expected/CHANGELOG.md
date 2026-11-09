# Golden Output Changes

## Pending — cross-surface annotation conformance (#12925)

Add the existing finding projection as `sarif.json` and `github.txt` under
RIPR-SPEC-0008 cross-surface conformance. Class, canonical identity,
configured severity and refusal guidance come from the existing producer;
the check/human goldens and analyzer behavior are unchanged.

## Pending — asserting_same_file_test_positive_control (1)

Reason:
RIPR-SPEC-0108 new positive-control fixture asserting_same_file_test_positive_control (#10091)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — asserting_same_file_test_positive_control (2)

Reason:
RIPR-SPEC-0108 add marker test beside asserting test in positive control (#10091)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — asserting_same_file_test_positive_control (3)

Reason:
RIPR-SPEC-0108: fixture owner gains a caller (peek) so reach stays weak after #10160; marker class unchanged, line numbers shift

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (4)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (5)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (6)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (7)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (8)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (9)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (10)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (11)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — asserting_same_file_test_positive_control (12)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless asserting_same_file_test_positive_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
