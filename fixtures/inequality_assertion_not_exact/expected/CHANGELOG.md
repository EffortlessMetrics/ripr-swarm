# Golden Output Changes

## Pending — inequality_assertion_not_exact (1)

Reason:
RIPR-SPEC-0231 rule 1: assert_ne! reads relational_check/weak, not exact_value/strong

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (2)

Reason:
RIPR-SPEC-0231: re-bless after merging main's output-format changes (canonical gap id, relative file paths); assert_ne! still reads relational_check/weak

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (5)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (6)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (7)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (8)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (10)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — inequality_assertion_not_exact (11)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (12)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — inequality_assertion_not_exact (13)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless inequality_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
