# Golden Output Changes

## Pending — javascript_tap_global_expect (1)

Reason:
RIPR-SPEC-0085 and RIPR-SPEC-0108: #9187 tap cannot borrow an unimported ambient Jest expect

Command:
`cargo xtask goldens bless javascript_tap_global_expect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — javascript_tap_global_expect (2)

Reason:
RIPR-SPEC-0085: regenerate expected output after merging main (evidence_stages and preview note format)

Command:
`cargo xtask goldens bless javascript_tap_global_expect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
