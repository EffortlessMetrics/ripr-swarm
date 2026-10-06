# Golden Output Changes

## Pending — unsafe_boundary_probe (1)

Reason:
RIPR-SPEC-0168: new fixture pinning the unsafe_boundary static_unknown probe at a changed interior line

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (2)

Reason:
RIPR-SPEC-0168: multi-hunk redesign — ordinary probes beside the boundary probe and controls that enter the diff

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (3)

Reason:
RIPR-SPEC-0168: composition of PR witness integration with main #3579 facts/owner resolution — witness path (unchanged by merge) now completes for the field_construction probe, upgrading propagate stage to the same Complete-witness wording already pinned by 15 sibling fixtures; finding classification and confidence unchanged

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (4)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (5)

Reason:
RIPR-SPEC-0082/RIPR-SPEC-0122 wording owner change (PR #3978): Why lines re-derived from reach/observe stage state, preview notes use language display names with singular file counts, recovery detail lines end with exactly one period; mechanical re-render of unchanged fixture evidence

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (6)

Reason:
RIPR-SPEC-0001: a changed line inside a function no test reaches is no_static_path whatever its probe shape; static_unknown escalate-to-mutation advice no longer stands in for a missing test
RIPR-SPEC-0122: human-full carries per-finding drill-in commands (#4379); digest why-line names the incomplete stage; unreached static_unknown asks for a test first

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (7)

Reason:
RIPR-SPEC-0122: unreached static_unknown next step hedges macro and integration reach

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (8)

Reason:
RIPR-SPEC-0094: re-bless after merging main (#4425 observed-value caps); unreached-owner findings read no_static_path and new-function signature lines are not probed (#4428)

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (9)

Reason:
RIPR-SPEC-0122: digest Next step wraps instead of cutting the remedy; stop reasons carry a gloss; boxed-wrapper limitation text has no whitespace runs (#4323)
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (10)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (11)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - related-test and observed-value caps, digest missing-discriminator and related-test totals, Hidden block names omitted findings by file:line (class) with the all-base-side distinction (#4320); RIPR-SPEC-0152: all-base-side runs name base-side evidence instead of a lower-priority framing

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (12)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/human.txt`

## Pending — unsafe_boundary_probe (13)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## #5051 plain no-path guidance

Only the selected untyped no-path safe-action sentence changes. Static-limited state, selection, classification, full output and JSON remain unchanged.

## Pending — unsafe_boundary_probe (14)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (15)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — unsafe_boundary_probe (16)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless unsafe_boundary_probe --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
