# Golden Output Changes

## Pending — owner_return_pin_pointer_mask_rebound (1)

Reason:
RIPR-SPEC-0197 / #9478: all-bit pointer arguments do not exercise a mask after the owner shadows the operand with52. Current b9da static red exposes it while all4 neutral/narrower one-test native controls pass; repaired c837 pointer suite passes all29static/58native controls. Preserve the original assertions and selected return-value nonpromotion contract.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
