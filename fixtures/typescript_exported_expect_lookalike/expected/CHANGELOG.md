# Golden Output Changes

## Pending — typescript_exported_expect_lookalike (1)

Reason:
RIPR-SPEC-0243 rule 3: exported local expect declarations retain their local identity and cannot supply runner oracle credit; four related tests stay unknown and the changed return stays weakly_exposed.

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — compact output matches the candidate renderer

Reason:
RIPR-SPEC-0243 rule 3: retain the unknown oracle and four related tests while
refreshing compact wrapping, glossary and no-canonical-gap guidance already
present on the pinned basis. JSON and full human output are unchanged.

Producer:
`cargo xtask fixtures typescript_exported_expect_lookalike`

Updated:
- `expected/human.txt`

## Pending — typescript_exported_expect_lookalike (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (3)

Reason:
RIPR-SPEC-0088: TypeScript preview check JSON states top-level canonical_gap_id (#10709)

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (4)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (5)

Reason:
RIPR-SPEC-0088: related-test selection ties keep the first row, matching the check headline (#11228)

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (6)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (7)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — typescript_exported_expect_lookalike (8)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless typescript_exported_expect_lookalike --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
