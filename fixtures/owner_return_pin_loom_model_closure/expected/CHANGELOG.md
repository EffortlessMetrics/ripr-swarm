# Golden Output Changes

## Pending — owner_return_pin_loom_model_closure (1)

Reason:
RIPR-SPEC-0240 #11543: a loom::model / shuttle::check_* runner closure is an analyzer limit, not a test gap

Command:
`cargo xtask goldens bless owner_return_pin_loom_model_closure --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — owner_return_pin_loom_model_closure (2)

Reason:
RIPR-SPEC-0122: plain wording for the Evidence stage name and the stage and state legend in the check digest (#10692)

Command:
`cargo xtask goldens bless owner_return_pin_loom_model_closure --reason "..."`

Updated:
- `expected/human.txt`
