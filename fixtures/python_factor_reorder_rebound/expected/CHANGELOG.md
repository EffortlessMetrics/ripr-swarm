# Golden Output Changes

## Pending — python_factor_reorder_rebound (1)

Reason:
RIPR-SPEC-0238: a rebound table factor does not establish discrimination of a named multiplication swap.

Command:
`cargo xtask goldens bless python_factor_reorder_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_factor_reorder_rebound (2)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless python_factor_reorder_rebound --reason "..."`

Updated:
- `expected/human.txt`
