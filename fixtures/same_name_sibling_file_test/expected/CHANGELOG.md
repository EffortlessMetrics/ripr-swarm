# Golden Output Changes

## Pending — same_name_sibling_file_test (1)

Reason:
RIPR-SPEC-0172: a test whose bare call binds a sibling module's same-named function does not relate to the changed function (#6537)

Command:
`cargo xtask goldens bless same_name_sibling_file_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — same_name_sibling_file_test (2)

Reason:
RIPR-SPEC-0172: re-bless after merging origin/main; main no longer emits a static_unknown probe for the replaced old line and renders fixture paths relative; call-identity verdicts unchanged

Command:
`cargo xtask goldens bless same_name_sibling_file_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
