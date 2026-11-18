# Golden Output Changes

## Pending — rust_predicate_appended_char_arm (1)

Reason:
RIPR-SPEC-0001: an input equal to an appended integer or char equality arm reaches the change, and an appended-arm tail carries it to the return (#15027)

Command:
`cargo xtask goldens bless rust_predicate_appended_char_arm --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending - rust_predicate_appended_char_arm (2)

Reason:
RIPR-SPEC-0197: #14194 guard-operand agreement makes a test whose pinned call leaves the appended arm at its identity value a weak oracle, not strong

Command:
`cargo xtask goldens bless rust_predicate_appended_char_arm --reason "..."` 

Updated:
- `expected/check.json`
- `expected/human-full.txt`
