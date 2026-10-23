# Golden Output Changes

## Pending — typescript_local_registration (1)

Reason:
RIPR-SPEC-0243 #8959: local no-op registrations cannot register boundary assertions; exact NoStaticPath independent honesty guard

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_local_registration (2)

Reason:
RIPR-SPEC-0243 #8959: include full human oracle evidence for the local-registration non-credit honesty guard

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_local_registration (3)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_local_registration (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_local_registration (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
