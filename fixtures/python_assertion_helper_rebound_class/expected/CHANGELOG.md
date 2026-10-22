# Golden Output Changes

## Pending — python_assertion_helper_rebound_class (1)

Reason:
RIPR-SPEC-0238: field-plus-literal helper credit preserves receiver, callable and expectation identity; native surviving-mutant controls and RIPR-SPEC-0108 reject false promotions (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_rebound_class --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
