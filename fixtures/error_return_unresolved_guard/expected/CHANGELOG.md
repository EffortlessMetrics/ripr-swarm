# Golden Output Changes

## Pending — error_return_unresolved_guard (1)

Reason:
RIPR-SPEC-0158 #1579: bless precise guard-unresolved limitation output (infection_unknown, typed limitation, no impossible prescription)

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_return_unresolved_guard (2)

Reason:
RIPR-SPEC-0158: ripr-main merge sync re-bless to swarm analyzer output; ripr-side behavior not carried, see merge PR residuals

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_return_unresolved_guard (3)

Reason:
RIPR-SPEC-0122 plain-words check screen: ripr-origin fixture re-blessed from the merged producer after merging main again (#2766); the Evidence line's last stage is labelled assertion, wording only, verdicts unchanged

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — error_return_unresolved_guard (4)

Reason:
RIPR-SPEC-0158: preserve public ripr#1579 by naming the actual AST guarded error producer edge, retaining infection_unknown and withholding an impossible assertion/input prescription; current/discarded/overwritten/shadowed/unentered production controls are discriminating

Command:
`cargo xtask goldens bless error_return_unresolved_guard --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
