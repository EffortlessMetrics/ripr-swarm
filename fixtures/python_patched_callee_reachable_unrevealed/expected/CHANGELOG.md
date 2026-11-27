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

## Pending — python_patched_callee_reachable_unrevealed (3)

Reason:
RIPR-SPEC-0008: add SARIF and GitHub annotation goldens for the Python class covered by the cross-surface conformance test (#12925)

Command:
`cargo xtask goldens bless python_patched_callee_reachable_unrevealed --reason "..."`

Updated:
- `expected/sarif.json`
- `expected/github.txt`

## Pending — python_patched_callee_reachable_unrevealed (4)

Reason:
RIPR-SPEC-0122: #15336 unconfirmed-reach note severity, possible-gap State line, legend and Terms glosses

Command:
`cargo xtask goldens bless python_patched_callee_reachable_unrevealed --reason "..."`

Updated:
- `expected/human.txt`
