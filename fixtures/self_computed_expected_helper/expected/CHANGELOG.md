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
