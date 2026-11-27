# Golden Output Changes

## Pending — rust_alloc_vec_import_assertion (1)

Reason:
RIPR-SPEC-0197: a test module's use alloc::vec; import keeps assert_eq! credited (#15263)

Command:
`cargo xtask goldens bless rust_alloc_vec_import_assertion --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
