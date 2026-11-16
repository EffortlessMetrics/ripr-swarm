# Golden Output Changes

## Pending — cross-surface annotation conformance (#12925)

Add the existing finding projection as `sarif.json` and `github.txt` under
RIPR-SPEC-0008 cross-surface conformance. Class, canonical identity,
configured severity and refusal guidance come from the existing producer;
the check/human goldens and analyzer behavior are unchanged.

## Pending — atomic_ordering_swap_strong_object_oracle (1)

Reason:
RIPR-SPEC-0108 #9080: independent control for atomic Ordering-only edit with strong whole-object oracle

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (2)

Reason:
RIPR-SPEC-0108 #9080: independent control for atomic Ordering-only edit with strong whole-object oracle; diff hunk header corrected so only the store line (12) is changed

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (3)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (4)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (5)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (6)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (7)

Reason:
RIPR-SPEC-0122: state that no seam binds the finding and name the Next step instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (8)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (9)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (10)

Reason:
RIPR-SPEC-0122: #10691 plain wording for the default digest's missing-assertion entry and the catch-all change type

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (11)

Reason:
RIPR-SPEC-0122 plain wording for the check screen's whole-seam/untied-spot refusals (#11619)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (12)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (13)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (14)

Reason:
RIPR-SPEC-0122: describe classified spots without implying repair eligibility

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (15)

Reason:
RIPR-SPEC-0122: render the missing check in plain language

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (16)

Reason:
RIPR-SPEC-0122: drill-in and repair-start command lines carry consumer labels naming their id domain (#12431)

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (17)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — atomic_ordering_swap_strong_object_oracle (18)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless atomic_ordering_swap_strong_object_oracle --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
