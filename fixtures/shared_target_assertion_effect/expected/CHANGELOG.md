# Golden Output Changes

## Pending — shared_target_assertion_effect (1)

Reason:
RIPR-SPEC-0169 and RIPR-SPEC-0197: assertion admission is universal over approved target contexts for side effects and call deletion; a binary no-op cannot restore credit through collection matching. Native value and deletion contrasts retain standard controls.

Command:
`cargo xtask goldens bless shared_target_assertion_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt` (retained from the same native blessing run)
