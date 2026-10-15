# Golden Output Changes

## Pending — same_name_qualified_other_module (1)

Reason:
RIPR-SPEC-0172: a qualified call to another module's same-named function is not a direct call of the changed function (#6292)

Command:
`cargo xtask goldens bless same_name_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_qualified_other_module (2)

Reason:
RIPR-SPEC-0172: re-bless after merging origin/main; main no longer emits a static_unknown probe for the replaced old line and renders fixture paths relative; call-identity verdicts unchanged

Command:
`cargo xtask goldens bless same_name_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_qualified_other_module (3)

Reason:
RIPR-SPEC-0172: diff.patch drops its trailing blank context line (git diff --check); only the input identity digest changes

Command:
`cargo xtask goldens bless same_name_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_qualified_other_module (4)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless same_name_qualified_other_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
