# Golden Output Changes

## Pending — rust_trait_default_shadow (1)

Reason:
RIPR-SPEC-0108 and RIPR-SPEC-0197: pin compiled trait dispatch and the literal shadow/import oracle for #7175

Command:
`cargo xtask goldens bless rust_trait_default_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_trait_default_shadow (2)

Reason:
RIPR-SPEC-0108 and RIPR-SPEC-0197: refresh authentic input_identity after trimming the final patch context line for #7175; unchanged semantic finding and literal oracle

Command:
`cargo xtask goldens bless rust_trait_default_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
