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
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/human.txt`
