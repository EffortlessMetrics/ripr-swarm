# Golden Output Changes

## Pending — python_message_pin_local_control (1)

Reason:
RIPR-SPEC-0238: pin the #9185 two-step message-only raise control where a literal match pattern rejects the old message

Command:
`cargo xtask goldens bless python_message_pin_local_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
