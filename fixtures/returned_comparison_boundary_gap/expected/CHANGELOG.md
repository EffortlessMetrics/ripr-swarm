# Golden Output Changes

## Pending — returned_comparison_boundary_gap (1)

Reason:
RIPR-SPEC-0108: new fixture pins a returned bare comparison's missing items == 10 boundary as weakly_exposed (#9094)

Command:
`cargo xtask goldens bless returned_comparison_boundary_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
