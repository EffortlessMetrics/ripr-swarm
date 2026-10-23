# Golden Output Changes

## Pending — python_conditional_version_attribute_write (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: unchanged runtime assertion never observes the edited behavior; preserve conservative classification

Command:
`cargo xtask goldens bless python_conditional_version_attribute_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_conditional_version_attribute_write (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executed unchanged test cannot observe this edited callable or value; pin structural identity and reachability refusal

Command:
`cargo xtask goldens bless python_conditional_version_attribute_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
