# Golden Output Changes

## Pending — owner_return_pin_pointer_invisible_input (1)

Reason:
RIPR-SPEC-0197 / #9478: a typed pointer at input3 does not distinguish min5 from min6. Exact d938 regression failed Exposed versus WeaklyExposed while all16 one-test native controls behaved as expected. Repaired39d5 passes8 pointer tests with37 static cases and74 one-test native executions, plus6 accepted direct-call controls. Preserve the original assertion, count1 and nonpromotion contract.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_invisible_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_pointer_invisible_input (2)

Reason:
RIPR-SPEC-0122 and RIPR-SPEC-0197: Refresh the candidate-owned human snapshots for the
accepted Evidence stage/state legend (#9657, #9373 item 1). Exact main0071b67c and the
pointer candidate450cf627 emit the same five-line legend on all seven supplied inputs.
Only this bounded legend is added; check JSON, source inputs, diffs, nonpromotion
assertions and classifications are unchanged (#9478).

Command:
`cargo xtask goldens bless owner_return_pin_pointer_invisible_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
