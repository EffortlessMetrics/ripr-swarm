# Golden Output Changes

## Pending — ts_class_getter_factory_read (1)

Reason:
RIPR-SPEC-0243 decision 23 (#10402): a getter read on a receiver built by an exported factory of the owner class is related, weakly exposed at most

Command:
`cargo xtask goldens bless ts_class_getter_factory_read --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_class_getter_factory_read (2)

Reason:
RIPR-SPEC-0243 decision 23 (#10402): add the human-full projection the honesty corpus case reads

Command:
`cargo xtask goldens bless ts_class_getter_factory_read --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
