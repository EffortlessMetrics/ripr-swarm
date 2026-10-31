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
RIPR-SPEC-0122: #10382 repair-route-not-ready wording regeneration; the expected file predates the wording owner and main inherited a formatting-only drift unrelated to any in-flight lane

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_tests (4)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_tests (5)

Reason:
RIPR-SPEC-0122: plain-language repair-route refusal names the missing evidence without internal fact codes (#10644)

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192 receiving composition

- `human.txt`: name kind and reason on every listed hidden row, preserving currentness and the selected finding. JSON and registered full-human expectations are unchanged.
- Proof: this fixture passes on exact main `aeeeddacd1e24f3b04e713572717e7b172966431`; native receiving producer `a3946a7f1002ae6f069cd18fb4ed45454667b326` emitted the intended presentation before the expectation repair. No verdict or supported-repair claim is added.

## Pending — module_reach_super_import_in_file_tests (6)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_tests (7)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_tests --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
