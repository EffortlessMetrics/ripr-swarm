# Golden Output Changes

## Pending — python_adversarial_replaced_block_non_delta_operand (1)

Reason:
RIPR-SPEC-0028: #7556 pins the replacement-block non-delta-operand false exposed; the return of a two-line block pairs its own old line and reads weakly_exposed

Command:
`cargo xtask goldens bless python_adversarial_replaced_block_non_delta_operand --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_adversarial_replaced_block_non_delta_operand (2)

Reason:
RIPR-SPEC-0255: check discloses the loaded ripr.toml - human header gains a config: line and check JSON gains a top-level config_path member exactly when a config file loaded; fixture inputs carry ripr.toml, so both surfaces add the disclosure (#7536)

Command:
`cargo xtask goldens bless python_adversarial_replaced_block_non_delta_operand --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
