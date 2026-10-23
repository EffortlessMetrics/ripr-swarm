# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0172: root build.rs without a mount relates no test; build-script next step and closing note do not ask for a co-located test (#9173)

Command:
`cargo xtask goldens bless rust_build_script_no_test_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
