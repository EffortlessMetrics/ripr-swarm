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

## Pending — javascript_push_argument_length (2)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless javascript_push_argument_length --reason "..."`

Updated:
- `expected/human.txt`

## Pending — javascript_push_argument_length (3)

Reason:
RIPR-SPEC-0122: #15336 human-full Terms line stops claiming test reach for weakly_exposed and reachable_unrevealed

Command:
`cargo xtask goldens bless javascript_push_argument_length --reason "..."`

Updated:
- `expected/human-full.txt`
