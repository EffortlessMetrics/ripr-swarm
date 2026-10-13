# Golden Output Changes

## Pending — owner_return_pin_fallible_literal_message_escape (1)

Reason:
RIPR-SPEC-0197: #4478 refuse ensure message arguments that may swallow the failed equality

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_message_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_fallible_literal_message_escape (2)

Reason:
RIPR-SPEC-0197: #4478 refuse ensure message arguments that can swallow failed equality

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_message_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_fallible_literal_message_escape (3)

Reason:
RIPR-SPEC-0197: #6617 constant owner tail completes return propagation; verdict stays weakly_exposed (discriminator weak)

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_message_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
