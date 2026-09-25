# Golden Output Changes

## Pending — guarded_result_match_conditional_failure (1)

Reason:
RIPR-SPEC-0175: #3731 review fix F4 — bounded depth-0 terminal grammar; new fixture pins that conditional panics, unrelated unwraps, and closure-nested markers never credit (no_static_path)

Command:
`cargo xtask goldens bless guarded_result_match_conditional_failure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — guarded_result_match_conditional_failure (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless guarded_result_match_conditional_failure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — guarded_result_match_conditional_failure (3)

Reason:
RIPR-SPEC-0082/RIPR-SPEC-0122 wording owner change (PR #3978): Why lines re-derived from reach/observe stage state, preview notes use language display names with singular file counts, recovery detail lines end with exactly one period; mechanical re-render of unchanged fixture evidence

Command:
`cargo xtask goldens bless guarded_result_match_conditional_failure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
