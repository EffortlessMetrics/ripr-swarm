# Golden Output Changes

## Pending — rust_polling_helper_direct (1)

Reason:
RIPR-SPEC-0197: verified canonical helper observes the async owner; exact native wrong-return control fails its equality and independent honesty corpus pins exposed.

Command:
`cargo xtask goldens bless rust_polling_helper_direct --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.
