# Golden Output Changes

## Pending — python_effect_order_guarded_before_call (1)

Reason:
RIPR-SPEC-0260 and RIPR-SPEC-0108: withhold guarded credit before the owner call (#10178)

Command:
`cargo xtask goldens bless python_effect_order_guarded_before_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
