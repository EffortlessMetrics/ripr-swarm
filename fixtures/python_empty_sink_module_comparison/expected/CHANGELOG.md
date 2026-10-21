# Golden Output Changes
## Pending — python_empty_sink_module_comparison (1)

Reason:
RIPR-SPEC-0238 #7526: native valid comparison syntax has no call and its assignment mutant survives; retain the exact changed module finding as weakly_exposed/orthogonal without rewriting the relation scanner.

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — python_empty_sink_module_comparison (2)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_empty_sink_module_comparison (3)

Reason:
RIPR-SPEC-0002: re-bless human render copy/grammar fixes #7755 (double-colon labels), #7752 (a/an oracle articles), #7756 (trailing-period rule); formatting-only drift

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_empty_sink_module_comparison (4)

Reason:
RIPR-SPEC-0122: default human output speaks plain language (#8165)

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_empty_sink_module_comparison (5)

Reason:
RIPR-SPEC-0268: human evidence window glosses the unknown-sink row with its meaning (#8744)

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — python_empty_sink_module_comparison (6)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
## Pending — python_empty_sink_module_comparison (7)

Reason:
RIPR-SPEC-0122: #8729 names the repair-route refusal on the bounded check drill-down

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_empty_sink_module_comparison (8)

Reason:
RIPR-SPEC-0224 / #5491: Python related-test rows now carry truthful typed miss and why (explanation only; class, stages, order unchanged)

Command:
`cargo xtask goldens bless python_empty_sink_module_comparison --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
