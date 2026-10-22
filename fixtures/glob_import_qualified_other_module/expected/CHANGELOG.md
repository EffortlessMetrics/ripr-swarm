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

## Pending — glob_import_qualified_other_module (3)

Reason:
RIPR-SPEC-0122: no_static_path Weakness list no longer restates the Evidence block (#9033)

Command:
`cargo xtask goldens bless glob_import_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — glob_import_qualified_other_module (4)

Reason:
RIPR-SPEC-0122: the digest names an entry the Weakness list still shows, not a no_static_path restatement (#9033)

Command:
`cargo xtask goldens bless glob_import_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
