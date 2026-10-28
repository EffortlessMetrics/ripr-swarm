# Golden Output Changes

## Pending — python_comment_owner_credit (1)

Reason:
RIPR-SPEC-0238: an inert owner-name comment cannot observe a discarded return

Command:
`cargo xtask goldens bless python_comment_owner_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_comment_owner_credit (2)

Reason:
RIPR-SPEC-0238: retain full human output required by the independent expected-class assertion

Command:
`cargo xtask goldens bless python_comment_owner_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
