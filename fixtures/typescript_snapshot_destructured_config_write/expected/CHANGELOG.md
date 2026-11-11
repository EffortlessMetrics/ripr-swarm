# Golden Output Changes

## Pending — typescript_snapshot_destructured_config_write (1)

Reason:
RIPR-SPEC-0258: destructured config writes withhold strong committed-snapshot credit; independent honesty case pins medium

Command:
`cargo xtask goldens bless typescript_snapshot_destructured_config_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_snapshot_destructured_config_write (2)

Reason:
RIPR-SPEC-0258: include the full generated human projection required by the independent honesty assertions

Command:
`cargo xtask goldens bless typescript_snapshot_destructured_config_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
