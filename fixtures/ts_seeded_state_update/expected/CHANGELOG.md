# Golden Output Changes

## Pending — ts_seeded_state_update (1)

Reason:
RIPR-SPEC-0243: negative control for #10850, state the visible call does not show keeps the existing exposed verdict

Command:
`cargo xtask goldens bless ts_seeded_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_seeded_state_update (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless ts_seeded_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_seeded_state_update (3)

Reason:
RIPR-SPEC-0088: TypeScript preview check JSON states top-level canonical_gap_id (#10709)

Command:
`cargo xtask goldens bless ts_seeded_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_seeded_state_update (4)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless ts_seeded_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Note (#10700): `expected/human.txt` also moves its repair-refusal opening from the no-gap-id wording to `nothing to repair here`; that move is #10702/#10885 behavior, not caused by confidence_basis, and its transitional entry was removed in this bless.

## Pending — ts_seeded_state_update (5)

Reason:
RIPR-SPEC-0122 plain wording: TypeScript preview next step, observed check heading and rationale say check/assertion (#12124, #11619)

Command:
`cargo xtask goldens bless ts_seeded_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
