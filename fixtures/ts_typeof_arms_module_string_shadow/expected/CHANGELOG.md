# Golden Output Changes

## Pending — ts_typeof_arms_module_string_shadow (1)

Reason:
RIPR-SPEC-0243 rule 18: a module-scope rebinding of string refuses the String/JSON.stringify fold; the finding stays exposed (#12296)

Command:
`cargo xtask goldens bless ts_typeof_arms_module_string_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
