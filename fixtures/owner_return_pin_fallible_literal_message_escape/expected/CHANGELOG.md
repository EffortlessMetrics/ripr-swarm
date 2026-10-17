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

## Pending — owner_return_pin_fallible_literal_message_escape (4)

Reason:
RIPR-SPEC-0259: Next step for error_path/return_value weakly_exposed findings names the missing input when the strongest related oracle is already exact (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_message_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_fallible_literal_message_escape (5)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_message_escape --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
