# Golden Output Changes

## Pending — python_module_backslash_return (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: unchanged runtime assertion never observes the edited behavior; preserve conservative classification

Command:
`cargo xtask goldens bless python_module_backslash_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
