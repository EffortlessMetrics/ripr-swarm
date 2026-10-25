# Golden Output Changes

## Pending — python_factor_reorder (1)

Reason:
RIPR-SPEC-0238 #9815: numeric table rows cannot establish commutativity of generic named operands; native custom multiplication and unchanged-source row replay discriminate the former over-credit.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
