# Golden Output Changes

## Pending — owner_return_pin_test_module_shadow (1)

Reason:
RIPR-SPEC-0197: new fixture for #6905; test-module type shadow refuses the owner-return pin (1 weakly_exposed field-construction finding, exposed=0)

Command:
`cargo xtask goldens bless owner_return_pin_test_module_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_test_module_shadow (2)

Reason:
RIPR-SPEC-0197: test-local Window shadows production type, so window.clone() runs the derived clone, not the changed owner; reach is name-only weak instead of direct yes, verdict stays weakly_exposed with missing start discriminator (#6951)

Command:
`cargo xtask goldens bless owner_return_pin_test_module_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — owner_return_pin_test_module_shadow (3)

Reason:
RIPR-SPEC-0122 #5312: widen the before span-cut from predicates to every canonical-shape family (match arms exempt); human-full before: drops the `,` framing to match after

Command:
`cargo xtask goldens bless owner_return_pin_test_module_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
