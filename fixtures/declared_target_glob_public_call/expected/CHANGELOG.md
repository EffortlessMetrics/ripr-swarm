# Golden Output Changes

## Pending — declared_target_glob_public_call (1)

Reason:
RIPR-SPEC-0197: new fixture; a glob in a file composed only under another explicitly declared Cargo target no longer binds the library name in the test crate, so the crate-qualified exact assertion is credited and the owner return is exposed (#11123, #7415)

Command:
`cargo xtask goldens bless declared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
