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

## Pending — wildcard_owner_discard (3)

Reason:
RIPR-SPEC-0094: human.txt regenerated after merging main (check-screen wording)

Command:
`cargo xtask goldens bless wildcard_owner_discard --reason "..."`

Updated:
- `expected/human.txt`

## Pending — wildcard_owner_discard (4)

Reason:
RIPR-SPEC-0231: assert_eq!(matches!(Some(100), Some(_)), true) is a side-only pattern, so the related-test oracle reads smoke_only instead of exact_value; the finding stays weakly_exposed (#16290)

Command:
`cargo xtask goldens bless wildcard_owner_discard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
