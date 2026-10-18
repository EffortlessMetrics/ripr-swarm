# Golden Output Changes

## Pending — field_match_helper_taken (1)

Reason:
RIPR-SPEC-0229: implicit mutable field take invalidates the selecting write; the wrong Some return survives the independent native negative control

Command:
`cargo xtask goldens bless field_match_helper_taken --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
