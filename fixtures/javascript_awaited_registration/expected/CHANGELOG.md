# Golden Output Changes

## Pending — javascript_awaited_registration (1)

Reason:
RIPR-SPEC-0243 rule 1: exact awaited consumer of pinned Nano ID; native baseline/mutant/smoke contrast and CLI assertion invariant independently justify exposure.

Command:
`cargo xtask goldens bless javascript_awaited_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — javascript_awaited_registration (2)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless javascript_awaited_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — javascript_awaited_registration (3)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless javascript_awaited_registration --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
