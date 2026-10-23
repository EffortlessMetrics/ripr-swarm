# Golden Output Changes

## Pending — python_module_test_reader_replacement (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executed unchanged test cannot observe this edited callable or value; pin structural identity and reachability refusal

Command:
`cargo xtask goldens bless python_module_test_reader_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
