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

## Pending — typescript_trailing_setup_hook_argument (2)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless typescript_trailing_setup_hook_argument --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
