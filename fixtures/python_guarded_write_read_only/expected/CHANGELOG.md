# Golden Output Changes

## Pending — python_guarded_write_read_only (1)

Reason:
RIPR-SPEC-0260 / #9833 / #9868: actual native old/new and original/composed CLI red controls establish one intended field finding; AST write contexts distinguish retained reads from masking loop or call-result stores.

Command:
`cargo xtask goldens bless python_guarded_write_read_only --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
