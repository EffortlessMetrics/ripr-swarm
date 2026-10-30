# Golden Output Changes

## Pending — python_assertion_helper_method_rebinding (1)

Reason:
RIPR-SPEC-0238: field-plus-literal helper credit preserves receiver, callable and expectation identity; native surviving-mutant controls and RIPR-SPEC-0108 reject false promotions (#9594)

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_assertion_helper_method_rebinding (2)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: retain one weak finding for field-helper identity, literal-expected and activation controls through the landed resolver.

Command:
`cargo xtask goldens bless python_assertion_helper_method_rebinding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
