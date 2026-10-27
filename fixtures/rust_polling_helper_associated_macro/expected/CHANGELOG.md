# Golden Output Changes

## Pending — rust_polling_helper_associated_macro (1)

Reason:
RIPR-SPEC-0197: associated-item macros hide an intercepting method; independent native control starts one test but exits before completion, and honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_associated_macro --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.
