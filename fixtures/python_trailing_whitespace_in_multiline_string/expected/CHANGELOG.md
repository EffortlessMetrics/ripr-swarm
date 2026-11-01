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
#11210 follow-up: the human verify-command line uses the shell-labelled Bash form every other Python fixture already carries; formatting-only drift, one human.txt line, check.json unchanged

Command:
`cargo xtask goldens bless python_trailing_whitespace_in_multiline_string --reason "..."`

Updated:
- `expected/human.txt`

## Pending — python_trailing_whitespace_in_multiline_string (3)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_trailing_whitespace_in_multiline_string --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
