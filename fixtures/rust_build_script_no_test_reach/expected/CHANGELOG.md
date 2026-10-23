# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0172: root build.rs without a mount relates no test; build-script next step and closing note do not ask for a co-located test (#9173)

Command:
`cargo xtask goldens bless rust_build_script_no_test_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_build_script_no_test_reach (1)

Reason:
RIPR-SPEC-0122: #9373 item 1 selected Evidence legend coverage for the newly accepted fixture, preserving its current facts and all five stage values.

Operation:
Manual edit: inserted the existing five-line bounded Evidence legend immediately after the selected Evidence line.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_build_script_no_test_reach (2)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_build_script_no_test_reach --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
