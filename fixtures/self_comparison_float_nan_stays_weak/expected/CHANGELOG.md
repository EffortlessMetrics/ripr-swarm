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
