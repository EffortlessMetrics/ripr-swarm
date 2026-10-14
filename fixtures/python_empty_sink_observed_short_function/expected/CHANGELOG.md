# Golden Output Changes

## Pending — python_empty_sink_observed_short_function (1)

Reason:
RIPR-SPEC-0238 #7526: the unchanged exact f() assertion rejects the return mutant; preserve exposed/direct through the existing module-identified call branch.

Command:
`cargo xtask goldens bless python_empty_sink_observed_short_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_empty_sink_observed_short_function (2)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_empty_sink_observed_short_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_empty_sink_observed_short_function (3)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_empty_sink_observed_short_function --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
