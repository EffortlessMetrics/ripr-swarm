# Golden Output Changes

## Pending — error_path_operand_error_lexeme (1)

Reason:
RIPR-SPEC-0108: operand-position error lexeme cannot confirm ErrorPath (#5255)

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (2)

Reason:
RIPR-SPEC-0108: combined-tree refresh of operand-lexeme twin after main named related-test miss reasons (#5424); classification stays weakly_exposed / observation_unverified

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (3)

Reason:
RIPR-SPEC-0224, #5508: combined tree of #5255 and #5578; the operand-lexeme twin's strong row is observation_unconfirmed under #5578's producer. No verdict change.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
