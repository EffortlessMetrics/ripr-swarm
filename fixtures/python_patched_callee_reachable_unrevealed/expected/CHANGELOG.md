# Golden Output Changes

## Pending — python_patched_callee_reachable_unrevealed (1)

Reason:
RIPR-SPEC-0028: first Python reachable_unrevealed golden; every related test patches the changed callee rate_for itself so none runs the changed code (#13632)

Command:
`cargo xtask goldens bless python_patched_callee_reachable_unrevealed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_patched_callee_reachable_unrevealed (2)

Reason:
RIPR-SPEC-0028: re-blessed on current main, whose human summary now marks the reached-but-not-noticed count as held through a test link ripr could not confirm; the classification and check.json are unchanged (#12922)

Command:
`cargo xtask goldens bless python_patched_callee_reachable_unrevealed --reason "..."`

Updated:
- `expected/human.txt`
