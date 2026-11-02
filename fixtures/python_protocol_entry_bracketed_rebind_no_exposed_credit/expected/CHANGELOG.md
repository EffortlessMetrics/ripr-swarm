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
RIPR-SPEC-0122: compose the bounded Summary terms legend from #10004 with the protocol-entry fixture from #9207; add only its two short-human legend lines, preserving JSON, full-human output, stimulus and classification.

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (3)

Reason:
RIPR-SPEC-0319: human-full gains a Next line under static limitations and stop reasons

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (4)

Reason:
RIPR-SPEC-0122: digest missing-discriminator prose wraps instead of one_line clipping (#4323 parity); the ellipsized cut dropped the operative tail silently

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (5)

Reason:
RIPR-SPEC-0122: name the no-canonical-gap recovery instead of routing back to the gate (#9244)

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (6)

Reason:
RIPR-SPEC-0122: state only that the finding has no canonical gap, without a discriminator claim (#9244)

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (7)

Reason:
RIPR-SPEC-0122: define finding, unsuppressed, unknown, base-side and candidate line in the evidence legend (#9244)

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (8)

Reason:
RIPR-SPEC-0122: plain-language wording for the default and human-full check views (#10691); human text only, check.json unchanged

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (9)

Reason:
RIPR-SPEC-0028 (#9531): preserve the original receiver-rebinding subject and add six non-adjacent, outer/chained and inline for/with header controls. Independent actual-output assertions require seven related subjects, one static_unknown finding, zero exposed findings and no protocol_entry_call; retain the current main classification and rendering contracts.

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (10)

Reason:
RIPR-SPEC-0122: no-gap-ID refusal wording re-blessed now that the goldens gate compares it verbatim (#12003)

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (11)

Reason:
RIPR-SPEC-0118 / #10692: plain wording for seam and discriminator in default check and repair-start text; verdicts, identity and JSON fields unchanged

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`

## Pending — python_protocol_entry_bracketed_rebind_no_exposed_credit (12)

Reason:
RIPR-SPEC-0122: explain missing input or assertion honestly

Command:
`cargo xtask goldens bless python_protocol_entry_bracketed_rebind_no_exposed_credit --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
