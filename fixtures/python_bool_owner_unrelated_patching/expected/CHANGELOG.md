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
