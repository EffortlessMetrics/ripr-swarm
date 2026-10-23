# Golden Output Changes

## Pending — rust_build_script_no_test_reach (1)

Reason:
RIPR-SPEC-0172: new fixture, a root build.rs relates no test by name token (#9173)

Command:
`cargo xtask goldens bless rust_build_script_no_test_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_build_script_no_test_reach (2)

Reason:
RIPR-SPEC-0172: build.rs fixture avoids process spawn; build-script next step names build/inspect, not a co-located test (#9173)

Command:
`cargo xtask goldens bless rust_build_script_no_test_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
