# Golden Output Changes

## Pending — rust_sibling_effect_oracle (1)

Reason:
RIPR-SPEC-0094 #7794: parser-only equalities cannot confirm an aggregation effect by oracle kind

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_sibling_effect_oracle (2)

Reason:
RIPR-SPEC-0094 #7794: parser-only equalities remain unconfirmed for aggregation effects

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (3)

Reason:
RIPR-SPEC-0094: refresh human renderer wording after merging main; formatting-only

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (4)

Reason:
RIPR-SPEC-0094: merged main; reach summary no longer lists parser-only decoy tests and human output carries main's repair-not-ready line; verdicts unchanged

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (5)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (6)

Reason:
RIPR-SPEC-0197: effect assertions in the retained module need execution admission; json! at line 101 mentions format, so the line-374 assert_eq! is refused and the finding reads reachable_unrevealed (#8927)

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (7)

Reason:
RIPR-SPEC-0224: multi-line related-test oracle collapses to one human evidence row (#9165); formatting only, no verdict change

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (7)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (8)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence stage and state legend; preserve current analysis, JSON and human-full

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_sibling_effect_oracle (10)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_sibling_effect_oracle (11)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless rust_sibling_effect_oracle --reason "..."`

Updated:
- `expected/human.txt`
