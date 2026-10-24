# Golden Output Changes

## Pending — python_unreached_match_arm (1)

Reason:
RIPR-SPEC-0260: unreached match arm honesty-corpus control for #9429

Command:
`cargo xtask goldens bless python_unreached_match_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_unreached_match_arm (2)

Reason:
RIPR-SPEC-0260: add human-full golden for #9429 honesty controls

Command:
`cargo xtask goldens bless python_unreached_match_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unreached_match_arm (3)

Reason:
RIPR-SPEC-0122: #9373 item 1 legend composed with newer main fixture; preserve every other output byte

Command:
`cargo xtask goldens bless python_unreached_match_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unreached_match_arm (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — python_unreached_match_arm (5)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless python_unreached_match_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
