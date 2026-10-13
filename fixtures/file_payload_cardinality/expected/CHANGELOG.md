# Golden Output Changes

## Pending — file_payload_cardinality (1)

Reason:
RIPR-SPEC-0001 #6501: freeze file-mediated 128/129 cardinality as infection_unknown with no invented missing discriminator; direct caller and unrelated proximity row remain distinct.

Command:
`cargo xtask goldens bless file_payload_cardinality --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
