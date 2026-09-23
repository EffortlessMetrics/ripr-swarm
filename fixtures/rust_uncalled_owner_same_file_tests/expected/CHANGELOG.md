# Golden Output Changes

## Pending — rust_uncalled_owner_same_file_tests (1)

Reason:
RIPR-SPEC-0001: new fixture; an uncalled owner linked to tests only by same-file proximity has weak reach and is not exposed

Command:
`cargo xtask goldens bless rust_uncalled_owner_same_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
