# Golden Output Changes

## Pending — value_source_file_collision (1)

Reason:
RIPR-SPEC-0158 and #7805: preserve per-file source identity at colliding assertion lines without strengthening classification

Command:
`cargo xtask goldens bless value_source_file_collision --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
