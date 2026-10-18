# Golden Output Changes

## Pending — field_match_helper_mutable (1)

Reason:
RIPR-SPEC-0229: exact same-call helper mutable field selector exposes the changed Some arm; native positive control rejects the wrong return

Command:
`cargo xtask goldens bless field_match_helper_mutable --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
