# Golden Output Changes

## Pending — rust_uncalled_owner_same_file_tests (1)

Reason:
RIPR-SPEC-0001: new fixture; an uncalled owner linked to tests only by same-file proximity has weak reach and is not exposed

Command:
`cargo xtask goldens bless rust_uncalled_owner_same_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_uncalled_owner_same_file_tests (2)

Reason:
RIPR-SPEC-0001: fixture diff.patch drops blank context lines (git diff --check); only input_identity changes

Command:
`cargo xtask goldens bless rust_uncalled_owner_same_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_uncalled_owner_same_file_tests (3)

Reason:
RIPR-SPEC-0084: re-emit this branch's new fixture under the landed None-default base envelope (top-level base omitted, base_revision null, summary-first ordering); findings, reach, classes and confidences unchanged

Command:
`cargo xtask goldens bless rust_uncalled_owner_same_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_uncalled_owner_same_file_tests (4)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless rust_uncalled_owner_same_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
