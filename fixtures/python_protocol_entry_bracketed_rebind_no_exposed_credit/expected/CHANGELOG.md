# Golden Output Changes

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (1)

Reason:
RIPR-SPEC-0028: pin the #9178 bracketed-rebind protocol-entry fixture

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

Re-bless after merging main (#9722): the dunder-protocol abstention now reports
`static_unknown` with `dynamic_dispatch_unresolved` for this still-uncredited
rebind; the weak heuristic relation is unchanged.

Re-bless `expected/human.txt` after merging main (#9657): the default human report
now prints the five-line Evidence stage and state legend after Evidence. Only
those five lines are added; classification, stages and evidence are unchanged.

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (2)

Reason:
RIPR-SPEC-0122: propagate the summary-terms legend re-bless from #10004 to python_protocol_entry_bracketed_rebind_no_exposed_credit; the formatting-only flip was missed by the #10004 sweep and leaves main red on goldens check

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
