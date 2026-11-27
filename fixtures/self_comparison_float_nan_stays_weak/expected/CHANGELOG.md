# Golden Output Changes

## Pending — self_comparison_float_nan_stays_weak (1)

Reason:
RIPR-SPEC-0094: contrast for the #13037 self-comparison heuristic; a NaN-capable float copy comparison stays weak (#16227)

Command:
`cargo xtask goldens bless self_comparison_float_nan_stays_weak --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — self_comparison_float_nan_stays_weak (2)

Reason:
RIPR-SPEC-0094: #13037 review 7111459915, a self-comparison row says static analysis did not establish discrimination instead of 'too weak'; next step names verification or an independent expected value. Wording only; class and strength unchanged.

Command:
`cargo xtask goldens bless self_comparison_float_nan_stays_weak --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
