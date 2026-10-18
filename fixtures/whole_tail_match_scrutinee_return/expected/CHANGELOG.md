# Golden Output Changes

## Pending — whole_tail_match_scrutinee_return (1)

Reason:
RIPR-SPEC-0229: preserve the independently compiling non-local return bypass as weak; whole-result observation must not borrow an unreachable match result (#5500).

Command:
`cargo xtask goldens bless whole_tail_match_scrutinee_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
