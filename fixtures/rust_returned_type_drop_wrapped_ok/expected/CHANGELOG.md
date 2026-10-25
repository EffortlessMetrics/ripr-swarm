# Golden Output Changes

## Pending — rust_returned_type_drop_wrapped_ok (1)

Reason:
RIPR-SPEC-0197: a non-plain Drop record wrapped in Ok in exit position still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_returned_type_drop_wrapped_ok (2)

Reason:
RIPR-SPEC-0197: a non-plain Drop record wrapped in Ok in exit position still demotes to weakly_exposed

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (3)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (4)

Reason:
RIPR-SPEC-0122: human output drops the observation_unverified token

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (5)

Reason:
RIPR-SPEC-0122: explain bounded Evidence stage and state meanings for #9373 item 1; retain full-human and JSON contracts

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (6)

Reason:
RIPR-SPEC-0122: #9373 item 1 review repair; missing discriminators include absent assertions as well as inputs

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (7)

Reason:
RIPR-SPEC-0122: #9373 item 1 observation-gloss repair; report independent related test/assertion evidence without implying that the changed arm executes.

Operation:
Manual edit: replaced only the observation clause in the existing five-line bounded Evidence legend with `observation = related test/assertion evidence`.

Execution:
Source-only reviewed edit; no native fixture or goldens-bless command executed because local storage admission is unavailable.

Updated:
- `expected/human.txt`

## Pending — rust_returned_type_drop_wrapped_ok (8)

Reason:
RIPR-SPEC-0122: bounded evidence legend defines the Summary count terms (#9244 item 4)

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (9)

Reason:
Conditional ReturnValue guidance when an exact assertion exists (RIPR-SPEC-0259, #8954); aligned tail-return and semicolon effect fixtures (RIPR-SPEC-0094)

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — rust_returned_type_drop_wrapped_ok (10)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless rust_returned_type_drop_wrapped_ok --reason "..."`

Updated:
- `expected/human.txt`
