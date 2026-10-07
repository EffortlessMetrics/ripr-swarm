# Golden Output Changes

## Pending — owner_return_pin_test_module_shadow (1)

Reason:
RIPR-SPEC-0197: new fixture for #6905; test-module type shadow refuses the owner-return pin (1 weakly_exposed field-construction finding, exposed=0)

Command:
`cargo xtask goldens bless owner_return_pin_test_module_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
