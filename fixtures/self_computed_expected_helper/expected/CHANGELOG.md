# Golden Output Changes

## Pending — self_computed_expected_helper (1)

Reason:
RIPR-SPEC-0094 (#5830): new fixture; an expected value computed through a test helper that calls the owner does not confirm observation

Command:
`cargo xtask goldens bless self_computed_expected_helper --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_helper (2)

Reason:
RIPR-SPEC-0094 #5830 (ported from #6624 onto current main): owner-scoped tokens confirm only in assertions bound to the owner; self-computed expected values are weak and unconfirmed. Output re-rendered in main's current format.

Command:
`cargo xtask goldens bless self_computed_expected_helper --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
