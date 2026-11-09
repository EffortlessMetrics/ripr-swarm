# Golden Output Changes

## Pending — cross-surface annotation conformance (#12925)

Add the existing finding projection as `sarif.json` and `github.txt` under
RIPR-SPEC-0008 cross-surface conformance. Class, canonical identity,
configured severity and refusal guidance come from the existing producer;
the check/human goldens and analyzer behavior are unchanged.

## Pending — compile_time_marker_test_no_observation (1)

Reason:
RIPR-SPEC-0108 new non-promotion fixture compile_time_marker_test_no_observation (#10091)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — compile_time_marker_test_no_observation (2)

Reason:
RIPR-SPEC-0108 new non-promotion fixture compile_time_marker_test_no_observation (#10091)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — compile_time_marker_test_no_observation (3)

Reason:
RIPR-SPEC-0108: fixture owner gains a caller (peek) so reach stays weak after #10160; marker class unchanged, line numbers shift

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (4)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (5)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (6)

Reason:
RIPR-SPEC-0122: plain-language repair-route refusal names the missing evidence without internal fact codes (#10644)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (7)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (8)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (9)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (10)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (11)

Reason:
RIPR-SPEC-0122: describe unestablished test reach without claiming owner invocation (#7330)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — compile_time_marker_test_no_observation (12)

Reason:
union-head re-bless after main merge: additive evidence_stages rows (RIPR-SPEC-0429)

Command:
`cargo xtask goldens bless compile_time_marker_test_no_observation --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
