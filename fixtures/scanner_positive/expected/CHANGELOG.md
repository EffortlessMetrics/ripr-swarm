# Golden Output Changes

## Pending

Reason:
RIPR-SPEC-0163: new fixture pinning the scanner-transition positive path (local-with-call-initializer operand jump, per-row scanner evaluation, boundary equality observed -> exposed)

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (2)

Reason:
RIPR-SPEC-0084: CheckInput default base is now None (was origin/main); --diff fixture envelopes honestly omit the inapplicable top-level base and record base_revision null. Only base/base_revision changed; findings, counts, and input_identity byte-identical.

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (3)

Reason:
RIPR-SPEC-0122: exposed findings default to info severity

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (4)

Reason:
RIPR-SPEC-0122: the analysis outcome and state lines lead with plain words; the id stays in parentheses

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (5)

Reason:
#4324 / RIPR-SPEC-0122: the bounded digest names all five stage states on one compact Evidence line (reach, infection, propagation, observation, discriminator) instead of letting the positional 2-line detail window hide the decisive stages behind a bare count; the detail window keeps its two lines and the remainder line now reads `N more detail line(s) in --format human-full`. JSON output is unchanged.

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/human.txt`

## Pending — scanner_positive (6)

Reason:
RIPR-SPEC-0122: bounded human surfaces disclose their windows - post-merge re-bless of the #4320 caps disclosure on the merged tree (origin/main digest Evidence line)

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (7)

Reason:
RIPR-SPEC-0122: #4322 summary header names all seven classes with canonical tokens and a shown/total denominator

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
## Pending — scanner_positive (8)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted. The three inherited config_identity drifts in wrapper_seam_callee_call_attribution reproduce on the base and are not part of this blessing.
RIPR-SPEC-0045: the Rust producer now populates Finding.canonical_gap and canonical_gap_id (#5268) with the gap:rust identity shape; additive output members only - golden-drift.json shows zero semantic flips (no added/removed findings, no class/oracle/stop-reason changes) across all 388 drifted surfaces

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (9)

Reason:
RIPR-SPEC-0002: issue #5996 routes finding locations through the shared workspace-relative owner (analysis::finding_location_text); the fixture-input root prefix (fixtures/<name>/input/...) no longer appears in check.json/human location fields, matching the root-relative form the LSP and MCP surfaces already emitted.
RIPR-SPEC-0045: review fix #5268 - quoted string/char literals are now byte-encoded in the Rust gap discriminator key so distinct predicates never share one identity; golden-drift.json shows zero semantic flips on the re-run

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (10)

Reason:
RIPR-SPEC-0002: merge of origin/main (#5268 canonical_gap for Rust and sibling updates) onto the #5996 workspace-relative location owner — expected files regenerate through the merged tree

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`

## Pending — scanner_positive (11)

Reason:
RIPR-SPEC-0116: the check drill-in binds --root to the resolved repository (#3948); goldens carry the <cwd>/ placeholder

Command:
`cargo xtask goldens bless scanner_positive --reason "..."`

Updated:
- `expected/check.json`
- `expected/human.txt`
