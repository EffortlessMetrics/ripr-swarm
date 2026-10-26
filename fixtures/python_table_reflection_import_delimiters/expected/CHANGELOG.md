# Golden Output Changes

## Pending — python_table_reflection_import_delimiters (1)

Reason:
RIPR-SPEC-0028: bridge import with brackets in comments or strings still links the re-exported module

Command:
`cargo xtask goldens bless python_table_reflection_import_delimiters --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
