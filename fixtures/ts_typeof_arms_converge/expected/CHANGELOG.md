# Golden Output Changes

## Pending — ts_typeof_arms_converge (1)

Reason:
RIPR-SPEC-0027: a typeof guard is judged on the branch each asserted input takes before and after the edit; converging arms withhold exposed and a kind-name edit credits an input of the old or new kind (#12296)

Command:
`cargo xtask goldens bless ts_typeof_arms_converge --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
