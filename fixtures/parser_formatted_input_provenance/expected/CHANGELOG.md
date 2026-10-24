# Golden Output Changes

## Pending — parser_formatted_input_provenance (1)

Reason:
RIPR-SPEC-0240: pin actual parser callers, formatted-input unknown and uncredited unrelated reason assertions (#7763)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — parser_formatted_input_provenance (2)

Reason:
RIPR-SPEC-0197: vec! is an admitted nested expansion (#7932), so the exact-value assert_eq! over Some(vec![..]) is credited; verdict class stays infection_unknown

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (5)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (6)

Reason:
RIPR-SPEC-0001: diff-check reach evidence names only tests whose relation establishes reach (#9033)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (7)

Reason:
RIPR-SPEC-0224: JSON evidence_path words a test with no assertion 'uses no oracle' (#8747)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (8)

Reason:
RIPR-SPEC-0001: an unmapped local boundary operand and a neutral input-free fallback name the literal inputs that would settle the unknown (#9276)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (9)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (10)

Reason:
RIPR-SPEC-0224: multi-line related-test oracle collapses to one human evidence row (#9165); formatting only, no verdict change

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (11)

Reason:
RIPR-SPEC-0122: infection_unknown next step names a concrete action (#9244 item 5); wording only, no classification change

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (12)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (13)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (14)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — parser_formatted_input_provenance (15)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (16)

Reason:
RIPR-SPEC-0224

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
