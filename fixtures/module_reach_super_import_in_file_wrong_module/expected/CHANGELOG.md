# Golden Output Changes

## Pending — module_reach_super_import_in_file_wrong_module (1)

Reason:
RIPR-SPEC-0159: #10279 in-file wrong-scope relative import counterexample

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_wrong_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_wrong_module (2)

Reason:
RIPR-SPEC-0159: re-blessed after merging main, #10258 wraps the human prose lines (#10279)

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_wrong_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_wrong_module (3)

Reason:
RIPR-SPEC-0159: #10279 counterexample; repair-start wording from main

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_wrong_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_in_file_wrong_module (4)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless module_reach_super_import_in_file_wrong_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## 2026-10-26 — RIPR-SPEC-0118 / #10192 receiving composition

- `human.txt`: name the hidden return-value kind/reason and use the shared named-edge guidance.
- `check.json`: change only the four recommended/suggested guidance strings for the two retained findings. Tests cannot close the static limitation; duplicate indexed public entries still withhold helper annotation. Classification, identity, canonical edges, prior evidence, route and repair admission are unchanged.
- Proof: this fixture passes on exact main `aeeeddacd1e24f3b04e713572717e7b172966431`; native receiving producer `a3946a7f1002ae6f069cd18fb4ed45454667b326` emitted the intended presentation before the expectation repair. No verdict or supported-repair claim is added.
