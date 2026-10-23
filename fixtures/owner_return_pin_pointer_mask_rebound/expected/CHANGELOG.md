# Golden Output Changes

## Pending — owner_return_pin_pointer_mask_rebound (1)

Reason:
RIPR-SPEC-0197 / #9478: all-bit pointer arguments do not exercise a mask after the owner shadows the operand with52. Current b9da static red exposes it while all4 neutral/narrower one-test native controls pass; repaired c837 pointer suite passes all29static/58native controls. Preserve the original assertions and selected return-value nonpromotion contract.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_mask_rebound (2)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Refresh the candidate-owned human snapshots for the
accepted Evidence stage/state legend (#9657, #9373 item 1). Exact main0071b67c and the
pointer candidate450cf627 emit the same five-line legend on all seven supplied inputs.
Only this bounded legend is added; check JSON, source inputs, diffs, nonpromotion
assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_mask_rebound (3)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Preserve the accepted Summary count-term legend
(#10004, #9244 item 4) in the seven pointer-owned bounded human snapshots. Exact
mainad1e3bb7 and pointer9342d1d5 native producers each emit these two lines on all seven
inputs. Only that accepted legend is added; check JSON, source inputs, diffs,
nonpromotion assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_rebound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
