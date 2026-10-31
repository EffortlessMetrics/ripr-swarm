# Golden Output Changes

## Pending — declared_target_glob_public_call (1)

Reason:
RIPR-SPEC-0197: new fixture; a glob in a file composed only under another explicitly declared Cargo target no longer binds the library name in the test crate, so the crate-qualified exact assertion is credited and the owner return is exposed (#11123, #7415)

Command:
`cargo xtask goldens bless declared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — declared_target_glob_public_call (2)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless declared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — declared_target_glob_public_call (3)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless declared_target_glob_public_call --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
