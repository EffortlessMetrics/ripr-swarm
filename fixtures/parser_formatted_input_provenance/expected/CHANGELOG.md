# Golden Output Changes

## Pending — parser_formatted_input_provenance (1)

Reason:
RIPR-SPEC-0240: pin actual parser callers, formatted-input unknown and uncredited unrelated reason assertions (#7763)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — parser_formatted_input_provenance (2)

Reason:
RIPR-SPEC-0197: vec! is an admitted nested expansion (#7932), so the exact-value assert_eq! over Some(vec![..]) is credited; verdict class stays infection_unknown

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (5)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — parser_formatted_input_provenance (6)

Reason:
RIPR-SPEC-0122: human source-value evidence line names its test file as path:line, and the window line reads 'read from related tests' (#8389)

Command:
`cargo xtask goldens bless parser_formatted_input_provenance --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
