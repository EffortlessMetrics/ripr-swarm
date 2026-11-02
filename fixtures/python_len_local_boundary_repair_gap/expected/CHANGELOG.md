# Golden Output Changes

## Pending — python_len_local_boundary_repair_gap (1)

Reason:
RIPR-SPEC-0028: new fixture pinning a once-bound len(param) local deriving the boundary equality so the repair card forms (#11103)

Command:
`cargo xtask goldens bless python_len_local_boundary_repair_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_len_local_boundary_repair_gap (2)

Reason:
RIPR-SPEC-0028 / RIPR-SPEC-0209 (#11050, #11160): bind the existing high-confidence
`tests/test_clip.py::test_keeps_short_text` Bash verification action to the
analyzed repository with `cd -P --` after integrating the len-local fixture.
This manual human-output reconciliation leaves fixture inputs,
`expected/check.json` and the `weakly_exposed` classification unchanged.

Updated:
- `expected/human.txt`

## Pending — python_len_local_boundary_repair_gap (3)

Reason:
RIPR-SPEC-0122 plain wording: non-Rust terminal says spot list / missing check, Python preview next step says missing check (#11619)

Command:
`cargo xtask goldens bless python_len_local_boundary_repair_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_len_local_boundary_repair_gap (4)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_len_local_boundary_repair_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_len_local_boundary_repair_gap (5)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_len_local_boundary_repair_gap --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
