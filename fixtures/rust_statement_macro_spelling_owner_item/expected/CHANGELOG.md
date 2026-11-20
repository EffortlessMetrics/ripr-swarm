# Golden Output Changes

## Pending — rust_statement_macro_spelling_owner_item (1)

Reason:
RIPR-SPEC-0108: evidence-promotion honesty fixture for the #8467 statement-macro item shape

Command:
`cargo xtask goldens bless rust_statement_macro_spelling_owner_item --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — rust_statement_macro_spelling_owner_item (2)

Reason:
RIPR-SPEC-0229 (#15359): repair no longer reads a match arm no test input selects as strongly gripped, so its no-repair-start reason matches check

Command:
`cargo xtask goldens bless rust_statement_macro_spelling_owner_item --reason "..."`

Updated:
- `expected/human.txt`
