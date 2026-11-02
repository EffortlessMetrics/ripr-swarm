# Golden Output Changes

## Pending — rust_contradiction_lone_test (1)

Reason:
RIPR-SPEC-0108: graduate the #10980/#11012 lone whole-test contradiction control into the honesty corpus (corpus case rust_contradiction_withhold_lone_test, issue #12412); new fixture, first bless from the pinned 1.95.0 Linux toolchain

Command:
`cargo xtask goldens bless rust_contradiction_lone_test --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
