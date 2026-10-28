# Golden Output Changes

## Pending — python_unsettled_indirect_parametrize (1)

Reason:
RIPR-SPEC-0238 and RIPR-SPEC-0108: an unsettled indirect= value routes the row through a fixture, so the boundary stays static_unknown (#10685)

Command:
`cargo xtask goldens bless python_unsettled_indirect_parametrize --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_unsettled_indirect_parametrize (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_unsettled_indirect_parametrize --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
