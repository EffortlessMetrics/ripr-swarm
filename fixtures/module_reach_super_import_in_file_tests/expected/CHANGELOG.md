# Golden Output Changes

## Pending — module_reach_super_import_in_file_tests (1)

Reason:
RIPR-SPEC-0159: #10279 in-file test module relative import control

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_tests (2)

Reason:
RIPR-SPEC-0159: #10279 control; after #9934 the private boolean return is confirmed through the boundary-pinned wrapper, so both probes are exposed

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_tests (3)

Reason:
RIPR-SPEC-0240: output wording only; #10291 added this fixture after #10382 reworded the not-ready repair-route line, so main's golden carried the old wording (no class change)

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
