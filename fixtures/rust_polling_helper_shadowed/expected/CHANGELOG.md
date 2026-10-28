# Golden Output Changes

## Pending — rust_polling_helper_shadowed (1)

Reason:
RIPR-SPEC-0197: a closure shadows the helper and discards the future; exact native wrong-return control passes so the independent honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.

## Pending — rust_polling_helper_shadowed (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
