# Golden Output Changes

## Pending — same_name_use_imported_owner (1)

Reason:
RIPR-SPEC-0197: a use import settles which same-named function a bare call pins (#6544)

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (2)

Reason:
RIPR-SPEC-0172: re-bless after merging origin/main; main no longer emits a static_unknown probe for the replaced old line and renders fixture paths relative; call-identity verdicts unchanged

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (3)

Reason:
RIPR-SPEC-0172: diff.patch drops its trailing blank context line (git diff --check); only the input identity digest changes

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (4)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_use_imported_owner (5)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless same_name_use_imported_owner --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
