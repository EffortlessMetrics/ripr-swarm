# Golden Output Changes

## Pending — typescript_snapshot_destructured_config_write (1)

Reason:
RIPR-SPEC-0258: destructured config writes withhold strong committed-snapshot credit; independent honesty case pins medium

Command:
`cargo xtask goldens bless typescript_snapshot_destructured_config_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
