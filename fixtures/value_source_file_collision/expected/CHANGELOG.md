# Golden Output Changes

## Pending — value_source_file_collision (1)

Reason:
RIPR-SPEC-0158 and #7805: preserve per-file source identity at colliding assertion lines without strengthening classification

Command:
`cargo xtask goldens bless value_source_file_collision --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — value_source_file_collision (2)

Reason:
RIPR-SPEC-0158 and #7805: re-bless PR-owned collision fixture against current main: plain-language human render (#8165) and exact-oracle gap next step (#7393); source_file metadata unchanged

Command:
`cargo xtask goldens bless value_source_file_collision --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
