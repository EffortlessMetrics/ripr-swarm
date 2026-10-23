# Golden Output Changes

## Pending — exact_any_membership_oracle (1)

Reason:
RIPR-SPEC-0231 rule 7 (#6991): baseline pinning exact_value/strong for exact-equality .any() membership

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (3)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (4)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (5)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — exact_any_membership_oracle (7)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — exact_any_membership_oracle (8)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless exact_any_membership_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
