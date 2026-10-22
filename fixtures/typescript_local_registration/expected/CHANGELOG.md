# Golden Output Changes

## Pending — typescript_local_registration (1)

Reason:
RIPR-SPEC-0243 #8959: local no-op registrations cannot register boundary assertions; exact NoStaticPath independent honesty guard

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_local_registration (2)

Reason:
RIPR-SPEC-0243 #8959: include full human oracle evidence for the local-registration non-credit honesty guard

Command:
`cargo xtask goldens bless typescript_local_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
