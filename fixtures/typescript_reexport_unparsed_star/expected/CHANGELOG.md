# Golden Output Changes

## Pending — typescript_reexport_unparsed_star (1)

Reason:
RIPR-SPEC-0095 and RIPR-SPEC-0108: unavailable star export facts cannot establish unique owner evidence; baseline CLI falsely reported exposed with one relation.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_reexport_unparsed_star (2)

Reason:
RIPR-SPEC-0108: retain the full human projection required by the independent expected-class assertion for unavailable star facts.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_reexport_unparsed_star (3)

Reason:
RIPR-SPEC-0095 and RIPR-SPEC-0108: integrate the landed #10906 owner-specific no-static-path guidance; retain the same one no_static_path finding, zero related tests, and independent refusal of positive credit. Only the two recommendation fields and their human projections changed.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_reexport_unparsed_star (4)

Reason:
RIPR-SPEC-0088, RIPR-SPEC-0095 and RIPR-SPEC-0108: integrate landed #10919 canonical_gap_id projection for this new TypeScript fixture. The identity is gap:typescript:typescript_preview:368e4177; one no_static_path finding, zero related tests and repair_packet_ready=false remain unchanged. Both human projections are byte-identical.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_reexport_unparsed_star (5)

Reason:
RIPR-SPEC-0113 and #10913: missing static relation is not universal test absence; inspect existing calls and import/export paths before adding a test if needed. Preserve classification, related-test credit and repair eligibility.

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_reexport_unparsed_star (6)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_reexport_unparsed_star (7)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless typescript_reexport_unparsed_star --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
