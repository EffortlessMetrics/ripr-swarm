# Golden Output Changes

## Pending — predicate_pairing_compound_assigned_binding (1)

Reason:
RIPR-SPEC-0186: post-let compound assignment voids a bound boundary name fail-closed; exact oracle on the mutated name does not pair (#7004)

Command:
`cargo xtask goldens bless predicate_pairing_compound_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_pairing_compound_assigned_binding (2)

Reason:
RIPR-SPEC-0186 (#7004): seed expected/human-full.txt so the honesty corpus pins the weakly_exposed projection; verdicts unchanged

Command:
`cargo xtask goldens bless predicate_pairing_compound_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_pairing_compound_assigned_binding (3)

Reason:
RIPR-SPEC-0122: adopt landed #7010 predicate before-span cut on merge tree (before cut to after's span)

Command:
`cargo xtask goldens bless predicate_pairing_compound_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — predicate_pairing_compound_assigned_binding (4)

Reason:
RIPR-SPEC-0122 #5471: stub route printed only when the resolver yields a stub, with --kind; refusal or nothing otherwise (merge re-bless)

Command:
`cargo xtask goldens bless predicate_pairing_compound_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
