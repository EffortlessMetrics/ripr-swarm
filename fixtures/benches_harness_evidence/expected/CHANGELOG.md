# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0153: initial bench-evidence fixture (#3283)

Command:
`cargo xtask goldens bless benches_harness_evidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0160: the additive git_candidate_subject identity field (null for ordinary runs) in the check JSON identity block

Command:
`cargo xtask goldens bless benches_harness_evidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — benches_harness_evidence (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless benches_harness_evidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
