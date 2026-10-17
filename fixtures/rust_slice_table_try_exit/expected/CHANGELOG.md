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
