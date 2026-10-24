# Golden Output Changes

## Pending — owner_return_pin_pointer_raw_shadow (1)

Reason:
RIPR-SPEC-0197 and RIPR-SPEC-0108: preserve the independently confirmed raw_shadow pointer-owner false-credit witness as one non-exposed return finding; matched neutral and changed-owner runtime tests both pass.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_raw_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_raw_shadow (2)

Reason:
RIPR-SPEC-0197 / #9478: update only the human repair-refusal explanation already accepted on exactmain12733. The exact-main native renderer reproduced the same4old snapshot mismatches before edit. All checkJSON, source inputs, diffs and nonpromotion oracles must remain unchanged.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_raw_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_raw_shadow (3)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Refresh the candidate-owned human snapshots for the
accepted Evidence stage/state legend (#9657, #9373 item 1). Exact main0071b67c and the
pointer candidate450cf627 emit the same five-line legend on all seven supplied inputs.
Only this bounded legend is added; check JSON, source inputs, diffs, nonpromotion
assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_raw_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_raw_shadow (4)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Preserve the accepted Summary count-term legend
(#10004, #9244 item 4) in the seven pointer-owned bounded human snapshots. Exact
mainad1e3bb7 and pointer9342d1d5 native producers each emit these two lines on all seven
inputs. Only that accepted legend is added; check JSON, source inputs, diffs,
nonpromotion assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_raw_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_raw_shadow (5)

Reason:
A test passes the owner as a typed function pointer that the pointer pin deliberately does not credit; with rust_indirect_owner_call_unresolved (RIPR-SPEC-0240, #9442) the gap is withheld as static_unknown instead of weakly_exposed. Still not credited, and the next step names the function-value reach.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_raw_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
