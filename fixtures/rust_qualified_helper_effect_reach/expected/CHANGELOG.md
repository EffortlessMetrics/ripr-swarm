# Golden Output Changes

## Pending — rust_qualified_helper_effect_reach (1)

Reason:
RIPR-SPEC-0159 qualified-hop reach must not borrow effect propagation; independent review and public API red control reproduced exposed before the refusal

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

The full human projection from that same fixture execution is also retained as
`expected/human-full.txt`; the family-specific honesty cases require it to
check that human output agrees with the two JSON propagation refusals.

## Pending — rust_qualified_helper_effect_reach (2)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: compose landed main fixture rows with 10192 hidden-row kind/reason naming and typed unresolved-edge guidance; JSON findings, classes, oracles and recommendations unchanged (golden-drift cosmetic-only)

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_helper_effect_reach (3)

Reason:
RIPR-SPEC-0122: check now prints one hand-written-test step when a gap has neither a repair start nor a stub, and only stub commands that produce a stub (#10710)
RIPR-SPEC-0122: incomplete evidence stage tokens render as plain words in the human repair advisory (#11025)

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_helper_effect_reach (4)

Reason:
RIPR-SPEC-0122: check human output drops the no-repair-route clause beside a repair advisory that names its own route or recovery (#10710)

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_qualified_helper_effect_reach (5)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless rust_qualified_helper_effect_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
