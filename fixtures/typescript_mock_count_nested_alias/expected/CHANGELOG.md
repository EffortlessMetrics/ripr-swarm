# Golden Output Changes

## Pending — typescript_mock_count_nested_alias (1)

Reason:
RIPR-SPEC-0243 decision 22 and RIPR-SPEC-0246 rule 9: an admitted nested unique count alias stays weak; original/equivalent/wrong-argument Node shim executions pass, while argument-pin controls reject actual 5 versus expected 8; only this new fixture is seeded.

Command:
`cargo xtask goldens bless typescript_mock_count_nested_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_mock_count_nested_alias (2)

Reason:
RIPR-SPEC-0243 decision 22: retain the same Node compatibility fixture as approved TypeScript input bridge.ts; runtime original/rewrite/wrong-argument controls pass, old analyzer falsely exposes, repaired analyzer stays weak; update only this new fixture input identity, with no policy or gate edit.

Command:
`cargo xtask goldens bless typescript_mock_count_nested_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

Receipt clarification: after renaming the retained bridge to TypeScript, the normalized check/human output bytes and input_identity were unchanged. This run updates the runtime-surface receipt; it does not establish a changed input identity.

## Pending — typescript_mock_count_nested_alias (3)

Reason:
RIPR-SPEC-0243 decision 22: integrate accepted main #10258 digest wrapping; retain the complete missing/advisory prose for this owned mock-count fixture; classification, source oracle, JSON and human-full bytes are unchanged.

Command:
`cargo xtask goldens bless typescript_mock_count_nested_alias --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
