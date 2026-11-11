# Golden Output Changes

## Pending — wildcard_owner_discard (1)

Reason:
RIPR-SPEC-0094: real wildcard discard retains one weak finding and its unrelated assertion; independent RIPR-SPEC-0108 pins reject false promotion.

Command:
`cargo xtask goldens bless wildcard_owner_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — wildcard_owner_discard (2)

Reason:
RIPR-SPEC-0108: include the complete human projection required by independent class and oracle invariants; classifications unchanged.

Command:
`cargo xtask goldens bless wildcard_owner_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
