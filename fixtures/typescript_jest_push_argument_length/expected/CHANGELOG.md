# Golden Output Changes

## Pending — typescript_jest_push_argument_length (1)

Reason:
RIPR-SPEC-0243: explicit Jest length-only assertions do not observe a changed pushed string; reviewed alongside discriminating index-zero and import-identity controls

Command:
`cargo xtask goldens bless typescript_jest_push_argument_length --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
