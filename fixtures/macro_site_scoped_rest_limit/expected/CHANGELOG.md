# Golden Output Changes

## Pending — macro_site_scoped_rest_limit (1)

Reason:
RIPR-SPEC-0240: scoped deferred definitions retain uncertainty (#8265)

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (2)

Reason:
RIPR-SPEC-0240 (#8265): preserve the foreign-glob scoped-site witness after #8388 trusts std/core. Use builtin alloc with an explicit extern declaration; inspected native output retains two static_unknown findings, both limitation codes, the unscoped alloc glob and no gap IDs. The extra declaration moves the changed bound to line 4.

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (3)

Reason:
RIPR-SPEC-0240: re-bless after merging main; wording only (assertion-not-credited reason, no-oracle phrasing, repair-start line), classes remain static_unknown with no gap IDs (#8265)

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (4)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — macro_site_scoped_rest_limit (7)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (8)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (9)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — macro_site_scoped_rest_limit (8)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless macro_site_scoped_rest_limit --reason "..."`

Updated:
- `expected/human.txt`
