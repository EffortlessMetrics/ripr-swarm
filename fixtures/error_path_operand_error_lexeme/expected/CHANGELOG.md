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
RIPR-SPEC-0108: re-bless operand-lexeme twin to the #5578 unconfirmed-observation wording inherited from main; classification unchanged (weakly_exposed / observation_unverified), formatting-only wording drift
RIPR-SPEC-0224, #5508: align this fixture with the b2f2a2154 observation_unconfirmed sentence; blessed pre-reword, pins the retired miss sentence. formatting_only 1-line flip per surface; no verdict change.

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — error_path_operand_error_lexeme (4)

Reason:
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless error_path_operand_error_lexeme --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
