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
