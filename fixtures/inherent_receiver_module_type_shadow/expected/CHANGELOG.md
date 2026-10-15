# Golden Output Changes

## Pending — inherent_receiver_module_type_shadow (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize the native module-shadow refusal and paired genuine owner control; retain both probes with low relation. Existing goldens and runtime labels are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_module_type_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — inherent_receiver_module_type_shadow (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_module_type_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
