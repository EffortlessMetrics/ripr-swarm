# Golden Output Changes

## Pending — python_message_pin_local_control (1)

Reason:
RIPR-SPEC-0238: pin the #9185 two-step message-only raise control where a literal match pattern rejects the old message

Command:
`cargo xtask goldens bless python_message_pin_local_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_message_pin_local_control (2)

Reason:
RIPR-SPEC-0182: Python finding ids move to the versioned v1- namespace (#11368); classification and finding count unchanged

Command:
`cargo xtask goldens bless python_message_pin_local_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
