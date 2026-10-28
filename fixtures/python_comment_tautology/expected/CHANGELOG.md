# Golden Output Changes

## Pending — python_comment_tautology (1)

Reason:
RIPR-SPEC-0238: an inert comment cannot distinguish identical assertion operands

Command:
`cargo xtask goldens bless python_comment_tautology --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_comment_tautology (2)

Reason:
RIPR-SPEC-0238: retain full human output required by the independent expected-class assertion

Command:
`cargo xtask goldens bless python_comment_tautology --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
