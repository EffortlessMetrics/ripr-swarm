# Golden Output Changes

## Pending — typescript_vitest_unpinned_literal_arm_return (1)

Reason:
RIPR-SPEC-0243: an exact assertion on another arm's result does not pin a changed literal arm return (#15306)

Command:
`cargo xtask goldens bless typescript_vitest_unpinned_literal_arm_return --reason "..."`

Updated:
- `expected/human-full.txt`

## Pending — typescript_vitest_unpinned_literal_arm_return (2)

Reason:
RIPR-SPEC-0243: the unpinned-arm line names only the changed arm's evidence (#15387 review)

Command:
`cargo xtask goldens bless typescript_vitest_unpinned_literal_arm_return --reason "..."`

Updated:
- `expected/check.json`
- `expected/human-full.txt`
