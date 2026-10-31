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

## Pending — typescript_parameter_default_setup_hook (2)

Reason:
RIPR-SPEC-0243: retain no_static_path and zero receiver evidence after conflict composition; align digest with accepted wrapping, legend and repair guidance from main (#10258, #10426, #10415)

Command:
`cargo xtask goldens bless typescript_parameter_default_setup_hook --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - typescript_parameter_default_setup_hook (3)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to observed/safe checks

Command:
`cargo xtask goldens bless typescript_parameter_default_setup_hook --reason "..."`

Updated:
- `expected/human.txt`
