# Golden Output Changes

## Pending — typescript_changed_boundary_dead_expected (1)

Reason:
RIPR-SPEC-0027 and RIPR-SPEC-0108: retain one weak predicate finding when the exact assertion expects neither boundary outcome; source-provenance removal was rejected by the production-path control.

Command:
`cargo xtask goldens bless typescript_changed_boundary_dead_expected --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — typescript_changed_boundary_dead_expected (2)

Reason:
RIPR-SPEC-0108: retain the full human projection of the independent weak-class and strong exact-value oracle invariants.

Command:
`cargo xtask goldens bless typescript_changed_boundary_dead_expected --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
