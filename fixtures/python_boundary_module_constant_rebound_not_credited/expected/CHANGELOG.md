# Golden Output Changes

## Pending — python_boundary_module_constant_rebound_not_credited (1)

Reason:
RIPR-SPEC-0028: new fixture for #10707 test-module constant boundary input

Command:
`cargo xtask goldens bless python_boundary_module_constant_rebound_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_boundary_module_constant_rebound_not_credited (2)

Reason:
RIPR-SPEC-0028: #10990 twice-bound test-module constant stays unresolved, so the gap keeps no discriminator and the preview finding carries no repair card; follow-up plain-word safe action for the unresolved-call decline (triage.rs #10990 branch)

Command:
`cargo xtask goldens bless python_boundary_module_constant_rebound_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
