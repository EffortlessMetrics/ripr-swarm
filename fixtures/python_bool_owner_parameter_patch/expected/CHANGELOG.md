# Golden Output Changes

## Pending — python_bool_owner_parameter_patch (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: preserve conservative classification and withhold bare-bool exact pins; native wrong-boundary control still passes

Command:
`cargo xtask goldens bless python_bool_owner_parameter_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_parameter_patch (2)

Reason:
RIPR-SPEC-0028: Linux-hosted rendering refresh for the patch-scope fixtures; hosted Rust-small gate observed formatting_only drift on the human surfaces

Command:
`cargo xtask goldens bless python_bool_owner_parameter_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_parameter_patch (3)

Reason:
RIPR-SPEC-0028: reconcile human goldens with the merged tree's rooted Bash verify rendering (#11050) and current renderer wording; regenerate from the pinned Linux toolchain so the hosted Rust-small gate reproduces these bytes

Command:
`cargo xtask goldens bless python_bool_owner_parameter_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_parameter_patch (4)

Reason:
RIPR-SPEC-0028: withhold the default human.txt golden. The merge-base delta gate of check-user-text (#11374) fails a new human.txt that carries banned plain-language terms (discriminator, seam) from the shared advisory legend, and wording that legend is separate work; check.json and human-full.txt keep pinning classification, oracle strength and the rendered evidence, as with python_message_pin_local_control

Command:
`cargo xtask goldens bless python_bool_owner_parameter_patch --reason "..."`

Updated:
- `expected/check.json` (unchanged)
- `expected/human-full.txt` (unchanged)
- `expected/human.txt` (removed)
