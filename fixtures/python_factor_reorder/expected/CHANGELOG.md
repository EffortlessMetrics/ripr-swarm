# Golden Output Changes

## Pending — python_factor_reorder (1)

Reason:
RIPR-SPEC-0238 #9815: numeric table rows cannot establish commutativity of generic named operands; native custom multiplication and unchanged-source row replay discriminate the former over-credit.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_factor_reorder (2)

Reason:
RIPR-SPEC-0238 #9815: request operands that distinguish multiplication order in both human and structured guidance; varying numeric rows does not discriminate the swap.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
