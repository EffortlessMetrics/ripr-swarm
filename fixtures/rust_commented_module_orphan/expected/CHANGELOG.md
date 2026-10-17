# Golden Output Changes

## Pending — rust_commented_module_orphan (1)

Reason:
RIPR-SPEC-0153: Cargo lists only the integration sentinel; the commented nested orphan must have no related test and no_static_path.

Command:
`cargo xtask goldens bless rust_commented_module_orphan --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_commented_module_orphan (2)

Reason:
RIPR-SPEC-0001: tail if/else sink resolved (#6781); propagation yes, class stays no_static_path, advisory confidence rises

Command:
`cargo xtask goldens bless rust_commented_module_orphan --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_commented_module_orphan (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless rust_commented_module_orphan --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_commented_module_orphan (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless rust_commented_module_orphan --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
