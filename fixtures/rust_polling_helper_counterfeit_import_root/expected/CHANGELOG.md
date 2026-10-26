# Golden Output Changes

## Pending — rust_polling_helper_counterfeit_import_root (1)

Reason:
RIPR-SPEC-0197: genuine core primitives do not verify a counterfeit std trait import; independent native control exits before completion, and honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.
