# Golden Output Changes

## Pending — glob_import_qualified_other_module (1)

Reason:
RIPR-SPEC-0172: a qualified call through a crate glob names the glob's module, not the changed same-named function (#9072)

Command:
`cargo xtask goldens bless glob_import_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — glob_import_qualified_other_module (2)

Reason:
RIPR-SPEC-0172: a qualified call through a crate glob names the glob's module, not the changed same-named function (#9072)

Command:
`cargo xtask goldens bless glob_import_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
