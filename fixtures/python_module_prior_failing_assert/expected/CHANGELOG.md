# Golden Output Changes

## Pending — python_module_prior_failing_assert (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executed unchanged test cannot observe this edited callable or value; pin structural identity and reachability refusal

Command:
`cargo xtask goldens bless python_module_prior_failing_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_module_prior_failing_assert (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: retain executed identity/reachability refusal and current main renderer stage/state legend for contribution-owned fixtures

Command:
`cargo xtask goldens bless python_module_prior_failing_assert --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
