# Golden Output Changes

## Pending — same_name_qualified_other_module (1)

Reason:
RIPR-SPEC-0172: a qualified call to another module's same-named function is not a direct call of the changed function (#6292)

Command:
`cargo xtask goldens bless same_name_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
