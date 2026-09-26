# Golden Output Changes

## Pending — ts_parse_depth_budget (1)

Reason:
RIPR-SPEC-0177: new fixture for the TypeScript parse-depth budget (#4101); initial golden for the typed expression_nesting_budget refusal and surviving classification

Command:
`cargo xtask goldens bless ts_parse_depth_budget --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
