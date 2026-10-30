# Golden Output Changes

## Pending — ts_one_line_if_dead_pin (1)

Reason:
RIPR-SPEC-0027: new fixture for a dead pin at the boundary of a one-line changed if (#10776); the scan read the enclosing block's returns and called the pin live

Command:
`cargo xtask goldens bless ts_one_line_if_dead_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_one_line_if_dead_pin (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless ts_one_line_if_dead_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_one_line_if_dead_pin (3)

Reason:
RIPR-SPEC-0088: TypeScript preview check JSON states top-level canonical_gap_id (#10709)

Command:
`cargo xtask goldens bless ts_one_line_if_dead_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_one_line_if_dead_pin (4)

Reason:
RIPR-SPEC-0087: plain TypeScript/JavaScript advisory wording for no-repair-start and blocked packet

Command:
`cargo xtask goldens bless ts_one_line_if_dead_pin --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
