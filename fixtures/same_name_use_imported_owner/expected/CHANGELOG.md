# Golden Output Changes

## Pending — same_name_use_imported_owner (1)

Reason:
RIPR-SPEC-0197: a use import settles which same-named function a bare call pins (#6544)

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (2)

Reason:
RIPR-SPEC-0172: re-bless after merging origin/main; main no longer emits a static_unknown probe for the replaced old line and renders fixture paths relative; call-identity verdicts unchanged

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (3)

Reason:
RIPR-SPEC-0172: diff.patch drops its trailing blank context line (git diff --check); only the input identity digest changes

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (4)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (5)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (6)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (7)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (8)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (9)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — same_name_use_imported_owner (10)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
