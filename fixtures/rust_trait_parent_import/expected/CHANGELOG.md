# Golden Output Changes

## Pending — rust_trait_parent_import (1)

Reason:
RIPR-SPEC-0108 and RIPR-SPEC-0197: pin compiled trait dispatch and the literal shadow/import oracle for #7175

Command:
`cargo xtask goldens bless rust_trait_parent_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_parent_import (2)

Reason:
RIPR-SPEC-0108 and RIPR-SPEC-0197: refresh authentic input_identity after trimming the final patch context line for #7175; unchanged semantic finding and literal oracle

Command:
`cargo xtask goldens bless rust_trait_parent_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_parent_import (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_trait_parent_import --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
