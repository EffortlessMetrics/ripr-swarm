# Golden Output Changes

## Pending — rust_indirect_owner_item_scope (1)

Reason:
RIPR-SPEC-0240: a nested item cannot hide the real owner value outside its block; native before/after assertions discriminate the edit (#10162).

Command:
`cargo xtask goldens bless rust_indirect_owner_item_scope --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
