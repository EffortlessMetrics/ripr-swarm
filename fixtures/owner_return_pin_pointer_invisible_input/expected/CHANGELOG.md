# Golden Output Changes

## Pending — owner_return_pin_pointer_invisible_input (1)

Reason:
RIPR-SPEC-0197 / #9478: a typed pointer at input3 does not distinguish min5 from min6. Exact d938 regression failed Exposed versus WeaklyExposed while all16 one-test native controls behaved as expected. Repaired39d5 passes8 pointer tests with37 static cases and74 one-test native executions, plus6 accepted direct-call controls. Preserve the original assertion, count1 and nonpromotion contract.

Command:
`cargo xtask goldens bless owner_return_pin_pointer_invisible_input --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
