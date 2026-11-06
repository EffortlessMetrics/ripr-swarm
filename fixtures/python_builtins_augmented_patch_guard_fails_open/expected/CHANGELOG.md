# Golden Output Changes

## Pending — python_builtins_augmented_patch_guard_fails_open (1)

Reason:
RIPR-SPEC-0260: pin #12592 augmented builtins-module patch fails open on isinstance guard

Command:
`cargo xtask goldens bless python_builtins_augmented_patch_guard_fails_open --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
