# Golden Output Changes

## Pending — rust_stub_unrelated_shadow_expected_value (1)

Reason:
RIPR-SPEC-0231: retain the weak finding when unrelated closure and match bindings shadow the unfilled expected local (#14868)

Command:
`cargo xtask goldens bless rust_stub_unrelated_shadow_expected_value --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending - rust_stub_unrelated_shadow_expected_value (2)

Reason:
RIPR-SPEC-0186: the repair explanation identifies the missing exact boundary assertion when the fixture has no paired oracle (#15664)

Command:
`cargo xtask goldens bless rust_stub_unrelated_shadow_expected_value --reason "..."`

Updated:
- `expected/human.txt`
