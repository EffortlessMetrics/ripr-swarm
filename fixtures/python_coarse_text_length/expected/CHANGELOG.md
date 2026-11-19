# Golden Output Changes

## Pending — python_coarse_text_length (1)

Reason:
RIPR-SPEC-0238: retain runtime-supported coarse projection non-promotion controls for #12126

Command:
`cargo xtask goldens bless python_coarse_text_length --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_coarse_text_length (2)

Reason:
RIPR-SPEC-0122: one blank line, not two, separates the More block from a trailing Note (#14318)

Command:
`cargo xtask goldens bless python_coarse_text_length --reason "..."`

Updated:
- `expected/human.txt`

## Pending — python_coarse_text_length (3)

Reason:
human-full: one blank line, not two, separates the last finding from a trailing Note (#14318)

Command:
`cargo xtask goldens bless python_coarse_text_length --reason "..."`

Updated:
- `expected/human-full.txt`
