# Golden Output Changes

## Pending — owner_return_pin_fallible_literal_inverted (1)

Reason:
RIPR-SPEC-0197: #4478 refuse an independent literal inconsistent with the indexed owner value

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_inverted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_fallible_literal_inverted (2)

Reason:
RIPR-SPEC-0197: #4478 refuse independent expectation inconsistent with indexed owner value

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_inverted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_fallible_literal_inverted (3)

Reason:
RIPR-SPEC-0197: #6617 constant owner tail completes return propagation; verdict stays weakly_exposed (discriminator weak)

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_inverted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_fallible_literal_inverted (4)

Reason:
RIPR-SPEC-0259: Next step for error_path/return_value weakly_exposed findings names the missing input when the strongest related oracle is already exact (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless owner_return_pin_fallible_literal_inverted --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
