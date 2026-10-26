# Golden Output Changes

## Pending — python_effect_order_module_before_call (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: withhold module-field credit before the owner call (#10178)

Command:
`cargo xtask goldens bless python_effect_order_module_before_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
