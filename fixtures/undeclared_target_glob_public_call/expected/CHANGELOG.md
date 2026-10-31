# Golden Output Changes

## Pending — undeclared_target_glob_public_call (1)

Reason:
RIPR-SPEC-0197: new fail-closed control for declared_target_glob_public_call; the same glob in a file no Cargo target is known to compile may bind the library name, so the crate-qualified call stays uncredited and the owner return stays weakly_exposed (#11123, #7415)

Command:
`cargo xtask goldens bless undeclared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
