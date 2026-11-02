# Golden Output Changes

## Pending — rust_polling_helper_counterfeit_import_root (1)

Reason:
RIPR-SPEC-0197: genuine core primitives do not verify a counterfeit std trait import; independent native control exits before completion, and honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.

## Pending — rust_polling_helper_counterfeit_import_root (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_counterfeit_import_root (3)

Reason:
RIPR-SPEC-0122: check now prints one hand-written-test step when a gap has neither a repair start nor a stub, and only stub commands that produce a stub (#10710)

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_counterfeit_import_root (4)

Reason:
RIPR-SPEC-0122: check prints an async-aware hand-written test step, withholds it when the repair view already counts the seam as noticed, and prints only stub commands that succeed (#10710)

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_counterfeit_import_root (5)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_counterfeit_import_root (6)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_counterfeit_import_root (7)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless rust_polling_helper_counterfeit_import_root --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
