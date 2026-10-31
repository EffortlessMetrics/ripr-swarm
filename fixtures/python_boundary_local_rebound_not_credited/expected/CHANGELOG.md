# Golden Output Changes

## Pending — python_boundary_local_rebound_not_credited (1)

Reason:
RIPR-SPEC-0028: new fixture for #10707

Command:
`cargo xtask goldens bless python_boundary_local_rebound_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_boundary_local_rebound_not_credited (2)

Reason:
RIPR-SPEC-0028: #10990 unresolved rebound boundary inputs stay unresolved, so the gap keeps no discriminator and the preview finding carries no repair card

Command:
`cargo xtask goldens bless python_boundary_local_rebound_not_credited --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
