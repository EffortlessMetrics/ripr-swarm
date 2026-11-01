# Golden Output Changes

## Pending — compile_time_assert_stop_reason (1)

Reason:
RIPR-SPEC-0002: #11547 a changed compile-time assertion reports stop reason compile_time_invariant instead of no_changed_rust_line

Command:
`cargo xtask goldens bless compile_time_assert_stop_reason --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
