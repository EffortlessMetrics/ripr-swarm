# Golden Output Changes

## Pending — python_module_same_line_prior_assert (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: retain executed identity/reachability refusal and current main renderer stage/state legend for contribution-owned fixtures

Command:
`cargo xtask goldens bless python_module_same_line_prior_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_module_same_line_prior_assert (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: require full human output for typed non-promotion honesty assertions; same conservative analyzer result

Command:
`cargo xtask goldens bless python_module_same_line_prior_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_same_line_prior_assert (3)

Reason:
RIPR-SPEC-0028: human output regenerated after merging main's summary-terms lines

Command:
`cargo xtask goldens bless python_module_same_line_prior_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_module_same_line_prior_assert (4)

Reason:
RIPR-SPEC-0319 / #9438: the human-full view now prints a next step under static limitations and stop reasons (#9449); fixture was added or last blessed before that change landed, so main's combined tree drifted. Only the new Next: lines change; classes, stages and findings are unchanged.

Command:
`cargo xtask goldens bless python_module_same_line_prior_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
