# Golden Output Changes

## Pending — python_package_alias_nested_root (1)

Reason:
RIPR-SPEC-0238 rule 33: retain the nested-root absolute-hop non-promotion witness for #13927

Command:
`cargo xtask goldens bless python_package_alias_nested_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_package_alias_nested_root (2)

Reason:
RIPR-SPEC-0122: one blank line, not two, separates the More block from a trailing Note (#14318)

Command:
`cargo xtask goldens bless python_package_alias_nested_root --reason "..."`

Updated:
- `expected/human.txt`

## Pending — python_package_alias_nested_root (3)

Reason:
human-full: one blank line, not two, separates the last finding from a trailing Note (#14318)

Command:
`cargo xtask goldens bless python_package_alias_nested_root --reason "..."`

Updated:
- `expected/human-full.txt`
