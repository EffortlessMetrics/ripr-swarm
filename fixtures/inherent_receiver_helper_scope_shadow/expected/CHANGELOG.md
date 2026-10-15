# Golden Output Changes

## Pending — inherent_receiver_helper_scope_shadow (1)

Reason:
RIPR-SPEC-0005 and RIPR-SPEC-0108: initialize native-verified function declaration shadows of a helper module and nominal receiver type; the genuine module helper remains admitted. No type arguments or return/value flow are resolved. Existing expectations and runtime labels are unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_helper_scope_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — inherent_receiver_helper_scope_shadow (2)

Reason:
RIPR-SPEC-0122: integrate main #8139 root-relative Hidden locations; only the fixture-root prefix changes to ./src, with source lines, classes and all JSON observations unchanged.

Command:
`cargo xtask goldens bless inherent_receiver_helper_scope_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
