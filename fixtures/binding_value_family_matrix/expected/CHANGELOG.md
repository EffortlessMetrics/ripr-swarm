# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0158: new fixture pinning the bounded value-transfer behavior (family matrix observes exact boundaries; unsupported chains fail closed by name)

Command:
`cargo xtask goldens bless binding_value_family_matrix --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0158: review round 2 — operand resolution hoisted per probe (provenance text unchanged in content but regenerated), the starts_with hunk made a real behavior change, and quote-aware splitting/char escapes/dedup refinements

Command:
`cargo xtask goldens bless binding_value_family_matrix --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0158: the body_of fallback is now observed by its own oracle (the else branch returns the changed default), and the starts_with hunk is a real behavior change; exact operands compare canonical renderings directly

Command:
`cargo xtask goldens bless binding_value_family_matrix --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending

Reason:
RIPR-SPEC-0158: the additive per-value provenance field now surfaces the evaluation chains and call sources the line-keyed assertion_texts map dropped (deferred #3295 follow-up)

Command:
`cargo xtask goldens bless binding_value_family_matrix --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
