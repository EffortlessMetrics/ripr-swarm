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
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — undeclared_target_glob_public_call (3)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
