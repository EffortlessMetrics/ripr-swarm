# Golden Output Changes

## Pending — typescript_config_relative_paths (1)

Reason:
RIPR-SPEC-0099: independently asserted literal return is reached through config-relative paths without baseUrl

Command:
`cargo xtask goldens bless typescript_config_relative_paths --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_config_relative_paths (2)

Reason:
RIPR-SPEC-0099: retain the current CLI full-human oracle evidence required by the honesty case

Command:
`cargo xtask goldens bless typescript_config_relative_paths --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
