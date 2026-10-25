# Golden Output Changes

## Pending — python_while_counter_past_bound (1)

Reason:
RIPR-SPEC-0028: issue 9733 native old and edited countdown(-2) both return [], so the empty-list oracle cannot discriminate the rebound boundary

Command:
`cargo xtask goldens bless python_while_counter_past_bound --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
