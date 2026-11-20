# Golden Output Changes

## Pending — javascript_tap_template_eval (1)

Reason:
RIPR-SPEC-0085 and RIPR-SPEC-0108: #9187 direct eval hidden in template interpolation must retain unknown oracles

Command:
`cargo xtask goldens bless javascript_tap_template_eval --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — javascript_tap_template_eval (2)

Reason:
RIPR-SPEC-0085: regenerate expected output after merging main (evidence_stages and preview note format)

Command:
`cargo xtask goldens bless javascript_tap_template_eval --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
