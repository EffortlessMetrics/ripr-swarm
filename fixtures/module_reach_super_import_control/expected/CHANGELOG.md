# Golden Output Changes

## Pending — module_reach_super_import_control (1)

Reason:
RIPR-SPEC-0159: #9971 relative import honesty fixture

Command:
`cargo xtask goldens bless module_reach_super_import_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_control (2)

Reason:
Conditional ReturnValue guidance from #10115 (RIPR-SPEC-0259) changed the recommended next step wording; control fixture golden re-blessed after merging main (#9971)

Command:
`cargo xtask goldens bless module_reach_super_import_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — module_reach_super_import_control (3)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless module_reach_super_import_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
