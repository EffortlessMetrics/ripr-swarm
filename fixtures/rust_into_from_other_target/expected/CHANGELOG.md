# Golden Output Changes

## Pending — rust_into_from_other_target (1)

Reason:
RIPR-SPEC-0001: a .into() that Rust does not dispatch to the changed From impl keeps its file-only relation (#15277)

Command:
`cargo xtask goldens bless rust_into_from_other_target --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
