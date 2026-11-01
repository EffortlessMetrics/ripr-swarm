# Golden Output Changes

## Pending — undeclared_target_glob_public_call (1)

Reason:
RIPR-SPEC-0197: new fail-closed control for declared_target_glob_public_call; the same glob in a file no Cargo target is known to compile may bind the library name, so the crate-qualified call stays uncredited and the owner return stays weakly_exposed (#11123, #7415)

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — undeclared_target_glob_public_call (2)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — undeclared_target_glob_public_call (3)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — undeclared_target_glob_public_call (4)

Reason:
RIPR-SPEC-0122 plain wording for the check screen's whole-seam/untied-spot refusals (#11619)
RIPR-SPEC-0152: base-side findings (base_deleted, moved_or_renamed) carry the not-a-candidate-edit-target refusal as their next step instead of test advice (#11905)

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
