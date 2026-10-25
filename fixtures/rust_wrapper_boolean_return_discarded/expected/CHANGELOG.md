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

## Pending — rust_wrapper_boolean_return_discarded (3)

Reason:
Compose main ad1e3bb's renderer with this candidate and include the two fixed
Summary terms lines added by #10004 after the stage/state legend. This aligns
the branch-only fixture with the same renderer as main's existing fixtures;
no classification, JSON, input or canonical ID change is intended. The prior
e68fbeb/1ac7968 replay covers the earlier renderer only; this composition needs
its own producer-bound replay before qualification.

Validation command:
Targeted producer-bound replay of `ripr check --root
fixtures/rust_wrapper_boolean_return_discarded/input --diff
fixtures/rust_wrapper_boolean_return_discarded/diff.patch --mode fast`, checking
JSON, human and human-full against the integrated renderer and unchanged JSON.

Updated:
- `expected/human.txt`

## Pending — rust_wrapper_boolean_return_discarded (4)

Reason:
RIPR-SPEC-0186: test moved to tests/api.rs so the guard keeps helper-chain reach while #10279 is open; classification unchanged (2 propagation_unknown)

Command:
`cargo xtask goldens bless rust_wrapper_boolean_return_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_wrapper_boolean_return_discarded (5)

Reason:
RIPR-SPEC-0186: human limitation line now wraps instead of truncating after #10258; classification unchanged

Command:
`cargo xtask goldens bless rust_wrapper_boolean_return_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_wrapper_boolean_return_discarded (6)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless rust_wrapper_boolean_return_discarded --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
