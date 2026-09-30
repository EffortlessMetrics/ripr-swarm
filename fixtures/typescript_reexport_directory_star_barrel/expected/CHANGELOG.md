# Golden Output Changes

## Pending — typescript_reexport_directory_star_barrel (1)

Reason:
RIPR-SPEC-0095 bounded re-export chains: new unjs/ufo regression fixture; a directory import of a star barrel (import from ../src, src/index.ts export star from ./utils) credits the changed withoutBase via re_export_chain_followed; initial golden bless

Command:
`cargo xtask goldens bless typescript_reexport_directory_star_barrel --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_reexport_directory_star_barrel (2)

Reason:
RIPR-SPEC-0122: exposed findings default to info severity (#4429, merged from main)

Command:
`cargo xtask goldens bless typescript_reexport_directory_star_barrel --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_reexport_directory_star_barrel (3)

Reason:
RIPR-SPEC-0046 RIPR-SPEC-0047: check JSON now carries a top-level source_subject stamp with the analysis-time content digests of the files a derived gap ledger names (#4544); no finding, classification, or human output changed.

Command:
`cargo xtask goldens bless typescript_reexport_directory_star_barrel --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_reexport_directory_star_barrel (4)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless typescript_reexport_directory_star_barrel --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
