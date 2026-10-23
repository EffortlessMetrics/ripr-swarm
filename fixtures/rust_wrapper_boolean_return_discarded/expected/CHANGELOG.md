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

## Pending — rust_wrapper_boolean_return_discarded (2)

Reason:
Align this branch-only fixture with the current upstream human renderer: include
the bounded Evidence stage/state legend and omit the observation_unverified
implementation marker from the human-full summary. Exact-base e68fbeb and
candidate 1ac7968 producer replays give identical human output and parsed JSON.
Both findings remain propagation_unknown with helper_result_not_forwarded;
check.json, fixture inputs, canonical gap IDs and runtime controls are unchanged.

Command:
Targeted producer-bound replay of `ripr check --root
fixtures/rust_wrapper_boolean_return_discarded/input --diff
fixtures/rust_wrapper_boolean_return_discarded/diff.patch --mode fast`, with
JSON, human and human-full surfaces and the shared fixture cwd/LF normalization.

Updated:
- `expected/human.txt`
- `expected/human-full.txt`
