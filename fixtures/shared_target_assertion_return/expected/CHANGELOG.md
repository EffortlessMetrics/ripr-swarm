# Golden Output Changes

## Pending — shared_target_assertion_return (1)

Reason:
RIPR-SPEC-0169 and RIPR-SPEC-0197: preserve all approved target contexts; the binary no-op assertion does not discriminate the changed return. Matched native standard/no-op controls independently establish the gap.

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt` (retained from the same native blessing run)
