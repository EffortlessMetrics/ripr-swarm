# Golden Output Changes

## Pending — python_effect_order_guarded_before_call (1)

Reason:
RIPR-SPEC-0260 and RIPR-SPEC-0108: withhold guarded credit before the owner call (#10178)

Command:
`cargo xtask goldens bless python_effect_order_guarded_before_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_effect_order_guarded_before_call (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_effect_order_guarded_before_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_effect_order_guarded_before_call (3)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_effect_order_guarded_before_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
