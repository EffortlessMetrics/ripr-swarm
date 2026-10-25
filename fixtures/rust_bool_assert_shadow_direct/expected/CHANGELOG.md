# Golden Output Changes

## Pending — rust_bool_assert_shadow_direct (1)

Reason:
RIPR-SPEC-0353: new honesty fixture, a shadowed assert_eq! keeps the bool chain infection_unknown (#10386)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_bool_assert_shadow_direct (2)

Reason:
RIPR-SPEC-0353: add human-full output to the honesty fixture, a shadowed assert_eq! keeps the bool chain infection_unknown (#10386)

Command:
`cargo xtask goldens bless rust_bool_assert_shadow_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
