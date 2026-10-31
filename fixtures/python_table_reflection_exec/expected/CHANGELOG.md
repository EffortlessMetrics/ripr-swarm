# Golden Output Changes

## Pending — python_table_reflection_exec (1)

Reason:
RIPR-SPEC-0028: reflective touch reached through a multiline import or evaluated code withholds table credit

Command:
`cargo xtask goldens bless python_table_reflection_exec --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_table_reflection_exec (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_table_reflection_exec --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_table_reflection_exec (3)

Reason:
#10697: plain wording for discriminator and seam in default human output; mirror the shared evidence/legend/no-seam rewording and reword TS preview discriminator labels to missing-check wording (safe, observed-advisory and exact-error-variant)

Command:
`cargo xtask goldens bless python_table_reflection_exec --reason "..."`

Updated:
- `expected/human.txt`
