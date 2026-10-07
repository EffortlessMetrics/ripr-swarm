# Golden Output Changes

## Pending — predicate_pairing_line_split_assigned_binding (1)

Reason:
RIPR-SPEC-0186: seed golden for the newline-split post-let reassignment; the detector now joins an unterminated statement tail with the next line so the stale binding voids and the verdict reads weakly_exposed naming same_test_pairing_missing (#7004)

Command:
`cargo xtask goldens bless predicate_pairing_line_split_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — predicate_pairing_line_split_assigned_binding (2)

Reason:
RIPR-SPEC-0186 (#7004): seed expected/human-full.txt so the honesty corpus pins the weakly_exposed projection; verdicts unchanged

Command:
`cargo xtask goldens bless predicate_pairing_line_split_assigned_binding --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
