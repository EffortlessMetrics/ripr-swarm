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
