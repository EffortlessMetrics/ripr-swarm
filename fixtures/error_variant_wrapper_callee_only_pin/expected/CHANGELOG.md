# Golden Output Changes

## Pending — error_variant_wrapper_callee_only_pin (1)

Reason:
RIPR-SPEC-0106: new #3700 BUG-1 negative fixture, callee-only pin stays weakly_exposed

Command:
`cargo xtask goldens bless error_variant_wrapper_callee_only_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
