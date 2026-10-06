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
