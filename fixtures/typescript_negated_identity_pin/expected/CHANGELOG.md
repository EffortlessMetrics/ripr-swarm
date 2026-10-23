# Golden Output Changes

## Pending — typescript_negated_identity_pin (1)

Reason:
RIPR-SPEC-0246 rule 1 (#8712): pin that a negated identity matcher cannot pin a copy-for-identity change to exposed

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_negated_identity_pin (2)

Reason:
RIPR-SPEC-0122 / #8729 (#8856): preserve the producer-owned NoCanonicalGap refusal advisory for this preview finding; no TypeScript verdict, JSON, or repair eligibility change

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_negated_identity_pin (3)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_negated_identity_pin (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless typescript_negated_identity_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_negated_identity_pin (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`
