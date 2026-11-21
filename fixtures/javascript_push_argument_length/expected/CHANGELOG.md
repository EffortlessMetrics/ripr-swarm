# Golden Output Changes

## Pending — javascript_push_argument_length (1)

Reason:
RIPR-SPEC-0108: new javascript honesty fixture modeled on typescript_push_argument_length; length does not observe the changed pushed value

Command:
`cargo xtask goldens bless javascript_push_argument_length --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
