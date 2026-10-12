# Golden Output Changes

## Pending — predicate_pairing_newline_split_reassigned_binding (1)

Reason:
RIPR-SPEC-0186 (#7042 item 4): newline-split reassignment voids a bound boundary name; exact oracle on the rebound name does not pair

Command:
`cargo xtask goldens bless predicate_pairing_newline_split_reassigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_pairing_newline_split_reassigned_binding (2)

Reason:
RIPR-SPEC-0186 (#7042 item 4): seed expected/human-full.txt so the honesty corpus pins the weakly_exposed projection; verdicts unchanged

Command:
`cargo xtask goldens bless predicate_pairing_newline_split_reassigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
