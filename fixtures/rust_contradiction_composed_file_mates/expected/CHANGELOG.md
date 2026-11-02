# Golden Output Changes

## Pending — rust_contradiction_composed_file_mates (1)

Reason:
RIPR-SPEC-0108: graduate the #12412 composed-shape control — unrelated file-mate composition cannot restore withheld contradiction credit (corpus case rust_contradiction_withhold_composed_file_mates); new fixture, first bless from the pinned 1.95.0 Linux toolchain

Command:
`cargo xtask goldens bless rust_contradiction_composed_file_mates --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
