# Golden Output Changes

## Pending — python_patched_callee_reachable_unrevealed (1)

Reason:
RIPR-SPEC-0028: first Python reachable_unrevealed golden; every related test patches the changed callee rate_for itself so none runs the changed code (#13632)

Command:
`cargo xtask goldens bless python_patched_callee_reachable_unrevealed --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
