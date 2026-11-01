# Golden Output Changes

## Pending — rust_polling_helper_shadowed (1)

Reason:
RIPR-SPEC-0197: a closure shadows the helper and discards the future; exact native wrong-return control passes so the independent honesty corpus requires static_unknown.

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Retained full rendering:
- `expected/human-full.txt` from the same successful fixture producer invocation above.
- The producer's `check.json` bytes match the blessed file exactly; one return-value subject and its independent literal classification were checked before copying the retained rendering.

## Pending — rust_polling_helper_shadowed (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_shadowed (3)

Reason:
RIPR-SPEC-0122: check now prints one hand-written-test step when a gap has neither a repair start nor a stub, and only stub commands that produce a stub (#10710)

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_shadowed (4)

Reason:
RIPR-SPEC-0122: check prints an async-aware hand-written test step, withholds it when the repair view already counts the seam as noticed, and prints only stub commands that succeed (#10710)

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_polling_helper_shadowed (5)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless rust_polling_helper_shadowed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
