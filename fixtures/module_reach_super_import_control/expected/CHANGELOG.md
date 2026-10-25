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
RIPR-SPEC-0186: exact wrapper boundary pins now confirm the private boolean whole-return twin (#9849, weakly_exposed -> exposed)

Command:
`cargo xtask goldens bless module_reach_super_import_control --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
