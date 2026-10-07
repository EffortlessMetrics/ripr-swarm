# Golden Output Changes

## Pending — self_computed_expected_value (1)

Reason:
RIPR-SPEC-0094 (#5830): new fixture; owner-scoped tokens and self-computed expected values do not confirm observation

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (2)

Reason:
RIPR-SPEC-0094 #5830 (ported from #6624 onto current main): owner-scoped tokens confirm only in assertions bound to the owner; self-computed expected values are weak and unconfirmed. Output re-rendered in main's current format.

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (3)

Reason:
RIPR-SPEC-0094: trailing blank context line trimmed from diff.patch (git diff --check); hunk header adjusted, verdicts unchanged.

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
