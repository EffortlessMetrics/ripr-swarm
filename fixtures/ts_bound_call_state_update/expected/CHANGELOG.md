# Golden Output Changes

## Pending — ts_bound_call_state_update (1)

Reason:
RIPR-SPEC-0243: a non-call mention of the owner keeps the exposed verdict (#10850)

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_bound_call_state_update (2)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_bound_call_state_update (3)

Reason:
RIPR-SPEC-0088: TypeScript preview check JSON states top-level canonical_gap_id (#10709)

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_bound_call_state_update (4)

Reason:
RIPR-SPEC-0026: additive confidence_basis and Why capped line for TypeScript/JavaScript preview findings (#10700)

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

Note (#10700): `expected/human.txt` also moves its repair-refusal opening from the no-gap-id wording to `nothing to repair here`; that move is #10702/#10885 behavior, not caused by confidence_basis, and its transitional entry was removed in this bless.

## Pending — ts_bound_call_state_update (5)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_bound_call_state_update (6)

Reason:
RIPR-SPEC-0118 / #10692: observed-assertion label replaces Discriminator in the default check digest; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_bound_call_state_update (7)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless ts_bound_call_state_update --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
