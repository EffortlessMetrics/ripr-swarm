# Golden Output Changes

## Pending — rust_same_test_disconnected_effect_oracle (1)

Reason:
RIPR-SPEC-0094: actual dispatch cannot lend kind-only credit to parser-before/parser-after or fresh-handle equality; 4/4 originals and compiling removals survive

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt` (same validated run)

## Pending — rust_same_test_disconnected_effect_oracle (2)

Reason:
RIPR-SPEC-0094: refresh human renderer wording after merging main; formatting-only

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (3)

Reason:
RIPR-SPEC-0094: merged main; reach summary no longer lists parser-only decoy tests and human output carries main's repair-not-ready line; verdicts unchanged

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (4)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (5)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 bounded Evidence legend on newer current-main fixtures; preserve current JSON and human-full

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (7)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_same_test_disconnected_effect_oracle (8)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (9)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_same_test_disconnected_effect_oracle (10)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless rust_same_test_disconnected_effect_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
