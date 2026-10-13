# Golden Output Changes

## Pending — python_empty_sink_module_comparison (1)

Reason:
RIPR-SPEC-0238 #7526: native valid comparison syntax has no call and its assignment mutant survives; retain the exact changed module finding as weakly_exposed/orthogonal without rewriting the relation scanner.

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
