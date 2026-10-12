# Golden Output Changes

## Pending — owner_parameter_token_coincidence (1)

Reason:
RIPR-SPEC-0094 (#5830): new fixture; owner-scoped tokens and self-computed expected values do not confirm observation

Command:
`cargo xtask goldens bless owner_parameter_token_coincidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_parameter_token_coincidence (2)

Reason:
RIPR-SPEC-0094: adopt main's observation_unverified wording after merge

Command:
`cargo xtask goldens bless owner_parameter_token_coincidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_parameter_token_coincidence (3)

Reason:
RIPR-SPEC-0094 #5830 (ported from #6624 onto current main): owner-scoped tokens confirm only in assertions bound to the owner; self-computed expected values are weak and unconfirmed. Output re-rendered in main's current format.

Command:
`cargo xtask goldens bless owner_parameter_token_coincidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_parameter_token_coincidence (4)

Reason:
RIPR-SPEC-0094: trailing blank context line trimmed from diff.patch (git diff --check); hunk header adjusted, verdicts unchanged.

Command:
`cargo xtask goldens bless owner_parameter_token_coincidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_parameter_token_coincidence (5)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless owner_parameter_token_coincidence --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
