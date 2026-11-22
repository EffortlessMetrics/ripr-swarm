# Golden Output Changes

## Pending — rust_lib_autolib_false_skipped (1)

Reason:
RIPR-SPEC-0356: an undeclared target under autobins/autolib = false yields no probe after target discovery #15892 (#15012)

Command:
`cargo xtask goldens bless rust_lib_autolib_false_skipped --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
