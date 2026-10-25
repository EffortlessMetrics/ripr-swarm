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
