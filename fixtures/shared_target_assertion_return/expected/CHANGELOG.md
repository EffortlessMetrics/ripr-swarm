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

## Pending — shared_target_assertion_return (2)

Reason:
RIPR-SPEC-0169 and RIPR-SPEC-0197: retain the accepted bounded-human Evidence legend from #9657 in the added #9969 shared-target assertion fixture; preserve its static classification and native standard/no-op controls.

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_return (3)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_return (4)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
