# Golden Output Changes

## Pending — typescript_reexport_directory_star_barrel (1)

Reason:
RIPR-SPEC-0095 bounded re-export chains: new unjs/ufo regression fixture; a directory import of a star barrel (import from ../src, src/index.ts export star from ./utils) credits the changed withoutBase via re_export_chain_followed; initial golden bless

Command:
`cargo xtask goldens bless typescript_reexport_directory_star_barrel --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
