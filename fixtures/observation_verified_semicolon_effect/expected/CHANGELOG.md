# Golden Output Changes

## Pending — observation_verified_semicolon_effect (1)

Reason:
Conditional ReturnValue guidance when an exact assertion exists (RIPR-SPEC-0259, #8954); aligned tail-return and semicolon effect fixtures (RIPR-SPEC-0094)

Command:
`cargo xtask goldens bless observation_verified_semicolon_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — observation_verified_semicolon_effect (2)

Reason:
Add human-full surface for the aligned semicolon effect fixture (#8954, RIPR-SPEC-0094)

Command:
`cargo xtask goldens bless observation_verified_semicolon_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — observation_verified_semicolon_effect (3)

Reason:
RIPR-SPEC-0122: state the missing evidence and the rerun for a not-ready repair route instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless observation_verified_semicolon_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — observation_verified_semicolon_effect (4)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless observation_verified_semicolon_effect --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — observation_verified_semicolon_effect (3)

Reason:
RIPR-SPEC-0118 / docs/OUTPUT_SCHEMA.md: #10192 displays hidden finding kind/reason, exact ambiguous seam choices, and evidence-specific unresolved-edge guidance; verdict, stage, identity and oracle fields stay unchanged (independently compared against actual JSON).

Command:
`cargo xtask goldens bless observation_verified_semicolon_effect --reason "..."`

Updated:
- `expected/human.txt`
