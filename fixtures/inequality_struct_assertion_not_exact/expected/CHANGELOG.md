# Golden Output Changes

## Pending — inequality_struct_assertion_not_exact (1)

Reason:
RIPR-SPEC-0231: assert_ne! against a struct literal stays whole_object_equality/weak under every probe family

Command:
`cargo xtask goldens bless inequality_struct_assertion_not_exact --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
