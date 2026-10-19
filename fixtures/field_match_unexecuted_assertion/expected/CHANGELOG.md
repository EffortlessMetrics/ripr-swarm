# Golden Output Changes

## Pending — field_match_unexecuted_assertion (1)

Reason:
RIPR-SPEC-0229: an unexecuted direct-record assertion is refused by shared execution admission; unchanged runtime test passes a wrong arm

Command:
`cargo xtask goldens bless field_match_unexecuted_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — field_match_unexecuted_assertion (2)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless field_match_unexecuted_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
