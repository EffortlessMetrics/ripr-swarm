# Golden Output Changes

## Pending — ts_one_line_if_dead_pin (1)

Reason:
RIPR-SPEC-0027: new fixture for a dead pin at the boundary of a one-line changed if (#10776); the scan read the enclosing block's returns and called the pin live

Command:
`cargo xtask goldens bless ts_one_line_if_dead_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
