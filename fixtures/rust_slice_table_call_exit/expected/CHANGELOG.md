# Golden Output Changes

## Pending — rust_slice_table_call_exit (1)

Reason:
RIPR-SPEC-0001: Initial adversarial fixture. Successful termination inside an assertion skips the empty row; independent public RED and disabled-guard runtime controls require infection_unknown without cardinality credit. No existing expected output or baseline changed.

Command:
`cargo xtask goldens bless rust_slice_table_call_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_call_exit (2)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_slice_table_call_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_slice_table_call_exit (3)

Reason:
RIPR-SPEC-0108: #7642 matched oracle rows with a proximity-only relation now carry no_call_path on exposed and unknown classes so sibling tests no longer render as unqualified oracle evidence

Command:
`cargo xtask goldens bless rust_slice_table_call_exit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
