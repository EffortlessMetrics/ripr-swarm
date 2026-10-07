# Golden Output Changes

## Pending — boundary_named_constant (1)

Reason:
RIPR-SPEC-0001: new fixture pinning named-constant boundaries: a same-file integer const resolves to its value, a test argument naming the const matches by identity, and an unresolvable const is reported as unknown instead of a missing discriminator

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (2)

Reason:
RIPR-SPEC-0122: omit zero-count languages, keep the empty-result caveat to empty runs, cut digest lines at word boundaries

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (3)

Reason:
RIPR-SPEC-0122: exposed findings default to info severity

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (4)

Reason:
RIPR-SPEC-0122: discriminator evidence line no longer says yes on findings that are not exposed

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (5)

Reason:
RIPR-SPEC-0122: bounded human check output leads the exposure line with the plain word the summary uses (weak, no path, unknown) before the schema value

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (6)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (7)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/human.txt`

## Pending — boundary_named_constant (8)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — boundary_named_constant (9)

Reason:
RIPR-SPEC-0122: check names the one-step ripr agent stub route for Rust value gaps (#5355)

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — boundary_named_constant (10)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (11)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (12)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (13)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — boundary_named_constant (14)

Reason:
RIPR-SPEC-0001: observed value context for parcels::BULK_ITEMS (a pub const) is constant, not enum_variant (#5357)

Command:
`cargo xtask goldens bless boundary_named_constant --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
