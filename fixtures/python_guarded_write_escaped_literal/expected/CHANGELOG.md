# Golden Output Changes

## Pending — python_guarded_write_escaped_literal (1)

Reason:
RIPR-SPEC-0260 / #9833 / #9868: native Python old/new controls establish the literal expected class; guard entry and explicit field retention determine this fixture, with one nonempty finding.

Command:
`cargo xtask goldens bless python_guarded_write_escaped_literal --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
