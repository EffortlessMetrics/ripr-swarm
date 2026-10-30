# Golden Output Changes

## Pending — python_bool_owner_formatted_patch (1)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: executable formatted patch must remain weakly_exposed; native wrong-result control still passes

Command:
`cargo xtask goldens bless python_bool_owner_formatted_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_bool_owner_formatted_patch (2)

Reason:
RIPR-SPEC-0028 and RIPR-SPEC-0108: preserve conservative classification and withhold bare-bool exact pins; native wrong-boundary control still passes

Command:
`cargo xtask goldens bless python_bool_owner_formatted_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_bool_owner_formatted_patch (3)

Reason:
RIPR-SPEC-0028: Linux-hosted rendering refresh for the patch-scope fixtures; hosted Rust-small gate observed formatting_only drift on the human surfaces

Command:
`cargo xtask goldens bless python_bool_owner_formatted_patch --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
