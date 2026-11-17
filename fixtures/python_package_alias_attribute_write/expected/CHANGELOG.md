# Golden Output Changes

## Pending — python_package_alias_attribute_write (1)

Reason:
RIPR-SPEC-0238 rule 33: retain the initializer attribute-write non-promotion witness for #13927

Command:
`cargo xtask goldens bless python_package_alias_attribute_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_package_alias_attribute_write (2)

Reason:
RIPR-SPEC-0122: one blank line, not two, separates the More block from a trailing Note (#14318)

Command:
`cargo xtask goldens bless python_package_alias_attribute_write --reason "..."`

Updated:
- `expected/human.txt`
