# Golden Output Changes

## Pending — typescript_trailing_setup_hook_argument (1)

Reason:
RIPR-SPEC-0243: suite arguments precede the callback; independently confirmed inert setup remains no_static_path (#10599)

Command:
`cargo xtask goldens bless typescript_trailing_setup_hook_argument --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - typescript_trailing_setup_hook_argument (2)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to observed/safe checks

Command:
`cargo xtask goldens bless typescript_trailing_setup_hook_argument --reason "..."`

Updated:
- `expected/human.txt`
