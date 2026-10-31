# Golden Output Changes

## Pending — python_truthiness_swap_inline_rebind_control (1)

Reason:
RIPR-SPEC-0028: #9186 review repair control, rebinding and indirect reach keep exposed

Command:
`cargo xtask goldens bless python_truthiness_swap_inline_rebind_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_truthiness_swap_inline_rebind_control (2)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless python_truthiness_swap_inline_rebind_control --reason "..."`

Updated:
- `expected/human.txt`
