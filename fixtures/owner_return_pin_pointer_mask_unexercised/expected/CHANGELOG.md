# Golden Output Changes

## Pending — owner_return_pin_pointer_mask_unexercised (1)

Reason:
RIPR-SPEC-0197 / #9478: retain the confirmed typed-pointer masked-input counterexample; 0x1234 leaves bit 0x80 clear so neutral 0xff and narrower 0x7f both return 0x34. Matched native controls execute one test each; all-bit controls kill the narrower mask. No exposure promotion is allowed.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_unexercised --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_mask_unexercised (2)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Refresh the candidate-owned human snapshots for the
accepted Evidence stage/state legend (#9657, #9373 item 1). Exact main0071b67c and the
pointer candidate450cf627 emit the same five-line legend on all seven supplied inputs.
Only this bounded legend is added; check JSON, source inputs, diffs, nonpromotion
assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_unexercised --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_mask_unexercised (3)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Preserve the accepted Summary count-term legend
(#10004, #9244 item 4) in the seven pointer-owned bounded human snapshots. Exact
mainad1e3bb7 and pointer9342d1d5 native producers each emit these two lines on all seven
inputs. Only that accepted legend is added; check JSON, source inputs, diffs,
nonpromotion assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_mask_unexercised --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
