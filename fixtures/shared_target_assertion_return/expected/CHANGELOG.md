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

## Pending — shared_target_assertion_return (5)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

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

## Pending — shared_target_assertion_return (6)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_return (7)

Reason:
RIPR-SPEC-0122: #9373 hidden rows name the unknown stage and gloss listed classes

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_return (8)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — shared_target_assertion_return (9)

Reason:
RIPR-SPEC-0152: base-side findings (base_deleted, moved_or_renamed) carry the not-a-candidate-edit-target refusal as their next step instead of test advice (#11905)

Command:
`cargo xtask goldens bless shared_target_assertion_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
