# Golden Output Changes

## Pending — rust_polling_helper_method_shadow (1)

Reason:
RIPR-SPEC-0197: an extension trait intercepts as_mut before the owner poll and exits zero without completing the declared test; independent honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_method_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.
