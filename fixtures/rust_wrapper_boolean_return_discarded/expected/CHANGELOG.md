# Golden Output Changes

## Pending — rust_wrapper_boolean_return_discarded (1)

Reason:
RIPR-SPEC-0186 #9849: exact wrapper assertions cannot observe a discarded private boolean return; both findings retain helper_result_not_forwarded

Command:
`cargo xtask goldens bless rust_wrapper_boolean_return_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
