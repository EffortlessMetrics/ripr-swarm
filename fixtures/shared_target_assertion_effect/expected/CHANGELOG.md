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

## Pending — shared_target_assertion_effect (2)

Reason:
RIPR-SPEC-0169 and RIPR-SPEC-0197: retain the accepted bounded-human Evidence legend from #9657 in the added #9969 shared-target assertion fixture; preserve its static classification and native standard/no-op controls.

Command:
`cargo xtask goldens bless shared_target_assertion_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_effect (3)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless shared_target_assertion_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
