# Golden Output Changes

## Pending — out_parameter_named_empty_no_promote (1)

Reason:
RIPR-SPEC-0351 #10058: named empty payload adds no data; native deletion control leaves prefix assertion unchanged; independent corpus forbids promotion

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — out_parameter_named_empty_no_promote (2)

Reason:
RIPR-SPEC-0351 #10058 composition: align this new fixture with accepted human summary definitions and repair-readiness guidance on receiving main; JSON classification and discriminator are byte-identical

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — out_parameter_named_empty_no_promote (3)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — out_parameter_named_empty_no_promote (4)

Reason:
RIPR-SPEC-0122: check now prints one hand-written-test step when a gap has neither a repair start nor a stub, and only stub commands that produce a stub (#10710)

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — out_parameter_named_empty_no_promote (5)

Reason:
RIPR-SPEC-0122: check human output drops the no-repair-route clause beside a repair advisory that names its own route or recovery (#10710)

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — out_parameter_named_empty_no_promote (6)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless out_parameter_named_empty_no_promote --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
