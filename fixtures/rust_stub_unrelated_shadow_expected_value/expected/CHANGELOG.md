# Golden Output Changes

## Pending — rust_stub_unrelated_shadow_expected_value (1)

Reason:
RIPR-SPEC-0231: retain the weak finding when unrelated closure and match bindings shadow the unfilled expected local (#14868)

Command:
`cargo xtask goldens bless rust_stub_unrelated_shadow_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
