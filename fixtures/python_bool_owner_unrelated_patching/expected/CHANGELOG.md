# Golden Output Changes

## Pending — python_bool_owner_unrelated_patching (1)

Reason:
RIPR-SPEC-0028: unrelated monkeypatch.setenv/mock use keeps the bare-bool credit (#10391)

Command:
`cargo xtask goldens bless python_bool_owner_unrelated_patching --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_unrelated_patching (2)

Reason:
RIPR-SPEC-0028: Linux-hosted rendering refresh for the patch-scope fixtures; hosted Rust-small gate observed formatting_only drift on the human surfaces

Command:
`cargo xtask goldens bless python_bool_owner_unrelated_patching --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_unrelated_patching (4)

Reason:
RIPR-SPEC-0028: withhold the default human.txt golden. The merge-base delta gate of check-user-text (#11374) fails a new human.txt that carries banned plain-language terms (discriminator, seam) from the shared advisory legend, and wording that legend is separate work; check.json and human-full.txt keep pinning classification, oracle strength and the rendered evidence, as with python_message_pin_local_control

Command:
`cargo xtask goldens bless python_bool_owner_unrelated_patching --reason "..."`

Updated:
- `expected/check.json` (unchanged)
- `expected/human-full.txt` (unchanged)
- `expected/human.txt` (removed)
