# Golden Output Changes

## Pending — rust_slice_table_try_exit (1)

Reason:
RIPR-SPEC-0001: discarded fallible closure exits after first nonempty owner input; the empty row is never called, so activation stays unresolved (#7553).

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_slice_table_try_exit (2)

Reason:
RIPR-SPEC-0001: retain all three new-fixture formats; skipped empty-row equality stays unresolved and honesty checks human-full (#7553).

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (4)

Reason:
RIPR-SPEC-0108: #7642 matched oracle rows with a proximity-only relation now carry no_call_path on exposed and unknown classes so sibling tests no longer render as unqualified oracle evidence

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (5)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (6)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (7)

Reason:
RIPR-SPEC-0122: human source-value evidence names the test file (path:line) only when the shown window spans more than one test file, and the window line reads 'read from related tests' (#8389)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (8)

Reason:
RIPR-SPEC-0001: diff-check reach evidence names only tests whose relation establishes reach (#9033)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (9)

Reason:
RIPR-SPEC-0224: human output words a test with no assertion 'uses no oracle' (#8747)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (10)

Reason:
RIPR-SPEC-0224: JSON evidence_path words a test with no assertion 'uses no oracle' (#8747)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (11)

Reason:
RIPR-SPEC-0122: route-not-ready refusal names its missing evidence (#9604 batch C)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (12)

Reason:
RIPR-SPEC-0122: infection_unknown next step names a concrete action (#9244 item 5); wording only, no classification change

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (13)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (14)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (15)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_slice_table_try_exit (16)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_try_exit (17)

Reason:
RIPR-SPEC-0224

Command:
`cargo xtask goldens bless rust_slice_table_try_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
