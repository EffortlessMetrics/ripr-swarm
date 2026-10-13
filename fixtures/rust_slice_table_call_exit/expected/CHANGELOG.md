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
