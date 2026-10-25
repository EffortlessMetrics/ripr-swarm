# Golden Output Changes

## Pending — python_dunder_truthiness_replaced_test (1)

Reason:
RIPR-SPEC-0238 rule 26: independent truthiness nonpromotion control for #9795

Command:
`cargo xtask goldens bless python_dunder_truthiness_replaced_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_dunder_truthiness_replaced_test (2)

Reason:
RIPR-SPEC-0238: preserve complete discriminator and analyzer-limit prose after landed renderer #10258; classifications and source scopes are unchanged

Command:
`cargo xtask goldens bless python_dunder_truthiness_replaced_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

Verification note: This update also restores two previously stale full-human Next guidance lines. The frozen BE75 producer and integrated renderer produce identical full-human bytes after the fixture runner's path normalization; classifications and assertions are unchanged.
