# Golden Output Changes

## Pending — python_warn_keyword_continuation_no_exposed_credit (1)

Reason:
RIPR-SPEC-0028: keyword continuation of warnings.warn is not exposed by a return pin (#16018)

Command:
`cargo xtask goldens bless python_warn_keyword_continuation_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
