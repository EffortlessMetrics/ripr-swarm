# Golden Output Changes

## Pending — module_reach_super_import_cfg_gated_test_module (1)

Reason:
RIPR-SPEC-0159: #9971 relative import honesty fixture

Command:
`cargo xtask goldens bless module_reach_super_import_cfg_gated_test_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_cfg_gated_test_module (2)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless module_reach_super_import_cfg_gated_test_module --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_cfg_gated_test_module (3)

Reason:
RIPR-SPEC-0118 / #10192: preserve exact canonical edge/route/non-claim and all prior evidence; add private-helper context only as separate limitation_helper_boundary evidence. Guidance retains the same displayed boundary, with verdict/stage/identity/oracle metadata unchanged.

Command:
`cargo xtask goldens bless module_reach_super_import_cfg_gated_test_module --reason "..."`

Updated:
- `expected/check.json`
