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
