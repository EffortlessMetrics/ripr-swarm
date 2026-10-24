# Golden Output Changes

## Pending — atomic_ordering_swap_strong_object_oracle (1)

Reason:
RIPR-SPEC-0108 #9080: independent control for atomic Ordering-only edit with strong whole-object oracle

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (2)

Reason:
RIPR-SPEC-0108 #9080: independent control for atomic Ordering-only edit with strong whole-object oracle; diff hunk header corrected so only the store line (12) is changed

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (3)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (6)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
