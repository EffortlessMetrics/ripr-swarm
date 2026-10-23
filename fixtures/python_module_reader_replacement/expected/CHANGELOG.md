# Golden Output Changes

## Pending — python_module_reader_replacement (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: unchanged runtime assertion never observes the edited behavior; preserve conservative classification

Command:
`cargo xtask goldens bless python_module_reader_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_module_reader_replacement (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executed unchanged test cannot observe this edited callable or value; pin structural identity and reachability refusal

Command:
`cargo xtask goldens bless python_module_reader_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
