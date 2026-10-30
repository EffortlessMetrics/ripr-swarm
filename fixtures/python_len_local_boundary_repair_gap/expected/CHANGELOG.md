# Golden Output Changes

## Pending — python_len_local_boundary_repair_gap (1)

Reason:
RIPR-SPEC-0028: new fixture pinning a once-bound len(param) local deriving the boundary equality so the repair card forms (#11103)

Command:
`cargo xtask goldens bless python_len_local_boundary_repair_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
