# Golden Output Changes

## Pending — python_guarded_statement_unreached_arm_no_exposed_credit (1)

Reason:
RIPR-SPEC-0260: #15970 pins an assignment in an arm no test input enters as weakly exposed

Command:
`cargo xtask goldens bless python_guarded_statement_unreached_arm_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
