# Golden Output Changes

## Pending — python_empty_sink_observed_short_function (1)

Reason:
RIPR-SPEC-0238 #7526: the unchanged exact f() assertion rejects the return mutant; preserve exposed/direct through the existing module-identified call branch.

Command:
`cargo xtask goldens bless python_empty_sink_observed_short_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
