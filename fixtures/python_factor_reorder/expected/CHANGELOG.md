# Golden Output Changes

## Pending — python_factor_reorder (1)

Reason:
RIPR-SPEC-0238 #9815: numeric table rows cannot establish commutativity of generic named operands; native custom multiplication and unchanged-source row replay discriminate the former over-credit.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_factor_reorder (2)

Reason:
RIPR-SPEC-0238 #9815: request operands that distinguish multiplication order in both human and structured guidance; varying numeric rows does not discriminate the swap.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_factor_reorder (3)

Reason:
RIPR-SPEC-0238 #9815 with landed #10258: retain the complete order-sensitive repair guidance in the compact digest. Independently derived wrap-only correction; JSON and human-full remain unchanged.

Command:
Manually reconcile the independently reviewed compact Missing discriminator block, then verify with `cargo xtask fixtures python_factor_reorder`.

Updated:
- `expected/human.txt`

## Pending — python_factor_reorder (4)

Reason:
RIPR-SPEC-0238 #9815 with landed #10426: include the renderer's unconditional Summary terms glossary additions. The weak verdict, order-sensitive remedy, JSON and human-full are unchanged.

Command:
Reconcile the source-reviewed glossary lines and compare all three surfaces using the frozen Rust+Python CLI produced on `0d024367`.

Updated:
- `expected/human.txt`

## Pending — python_factor_reorder (5)

Reason:
RIPR-SPEC-0238 #9815: expanded independent controls require four weak findings with order-sensitive guidance, including finite float rows, nested products and quoted hashes; baseline classifier assertions exposed the former errors.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_factor_reorder (6)

Reason:
RIPR-SPEC-0238: align human expectations with inherited current-main display vocabulary after native exact-main reproduction. Preserve all four weak findings, complete order-sensitive remedies, compact hidden-finding disclosure, and byte-identical structured JSON; no semantic verdict or threshold change.

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_factor_reorder (7)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: compose landed main fixture rows with 10192 hidden-row kind/reason naming and typed unresolved-edge guidance; JSON findings, classes, oracles and recommendations unchanged (golden-drift cosmetic-only)

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_factor_reorder (8)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_factor_reorder (9)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_factor_reorder --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
