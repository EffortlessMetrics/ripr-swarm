# Golden Output Changes

## Pending — python_conditional_version_annotated_write (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: unchanged runtime assertion never observes the edited behavior; preserve conservative classification

Command:
`cargo xtask goldens bless python_conditional_version_annotated_write --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
