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

## Pending — self_computed_expected_value (7)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (8)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (9)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (10)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (11)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (12)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (13)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — self_computed_expected_value (14)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (15)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_computed_expected_value (16)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless self_computed_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
