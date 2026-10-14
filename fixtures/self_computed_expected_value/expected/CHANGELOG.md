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

## Pending — self_computed_expected_value (4)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (5)

Reason:
RIPR-SPEC-0259: Next step for error_path/return_value weakly_exposed findings names the missing input when the strongest related oracle is already exact (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (6)

Reason:
RIPR-SPEC-0259: the exact-oracle Next step now requires an admitted strong oracle on a credited row; self-computed expectations and uncredited rows keep the family template (#7393); guidance text only, no class or evidence change

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
