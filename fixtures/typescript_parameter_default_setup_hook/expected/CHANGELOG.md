# Golden Output Changes

## Pending — typescript_parameter_default_setup_hook (1)

Reason:
RIPR-SPEC-0243 and RIPR-SPEC-0246: parameter-default hook writes resolve before body-local root facts; independent honesty invariant and live-hook contrasts retained.

Command:
`cargo xtask goldens bless typescript_parameter_default_setup_hook --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
