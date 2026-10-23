# Golden Output Changes

## Pending — python_conditional_walrus_replacement (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: unchanged runtime assertion never observes the edited behavior; preserve conservative classification

Command:
`cargo xtask goldens bless python_conditional_walrus_replacement --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
