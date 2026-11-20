# Golden Output Changes

## Pending — ts_typeof_arms_module_json_shadow (1)

Reason:
RIPR-SPEC-0243 rule 18: a module-scope rebinding of json refuses the String/JSON.stringify fold; the finding stays exposed (#12296)

Command:
`cargo xtask goldens bless ts_typeof_arms_module_json_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_typeof_arms_module_json_shadow (2)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; human rendering unchanged

Command:
`cargo xtask goldens bless ts_typeof_arms_module_json_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_typeof_arms_module_json_shadow (3)

Reason:
RIPR-SPEC-0423: additive evidence_stages rows and agent stages= line; no test attribution; human rendering unchanged

Command:
`cargo xtask goldens bless ts_typeof_arms_module_json_shadow --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — ts_typeof_arms_module_json_shadow (4)

Reason:
RIPR-SPEC-0122: one blank line, not two, separates the More block from a trailing Note (#14318)

Command:
`cargo xtask goldens bless ts_typeof_arms_module_json_shadow --reason "..."`

Updated:
- `expected/human.txt`

## Pending — ts_typeof_arms_module_json_shadow (5)

Reason:
RIPR-SPEC-0122: #15313 Summary names shared candidate lines and says No test gap when every finding reads noticed

Command:
`cargo xtask goldens bless ts_typeof_arms_module_json_shadow --reason "..."`

Updated:
- `expected/human.txt`
