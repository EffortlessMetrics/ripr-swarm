# Golden Output Changes

## Pending — module_reach_super_import_path_remap (1)

Reason:
RIPR-SPEC-0159: #9971 relative import honesty fixture

Command:
`cargo xtask goldens bless module_reach_super_import_path_remap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_path_remap (2)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless module_reach_super_import_path_remap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
