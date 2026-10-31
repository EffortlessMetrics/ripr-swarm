# Golden Output Changes

## Pending — python_trailing_whitespace_in_multiline_string (1)

Reason:
RIPR-SPEC-0028: new fixture for #10706 string-content negative

Command:
`cargo xtask goldens bless python_trailing_whitespace_in_multiline_string --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_trailing_whitespace_in_multiline_string (2)

Reason:
RIPR-SPEC-0028: reconcile human goldens with the merged tree's rooted Bash verify rendering (#11050) and current renderer wording; regenerate from the pinned Linux toolchain so the hosted Rust-small gate reproduces these bytes

Command:
`cargo xtask goldens bless python_trailing_whitespace_in_multiline_string --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
